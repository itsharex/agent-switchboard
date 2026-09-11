use super::super::{append_event, responses_complete, Frame};
use super::{merge_identity, parse_chat_frame, CallDelta, ChatSource, ResponseCall, TextOutput};
use crate::gateway::transform::{
    response::responses_reasoning_item, tool_names::parse_target_name, CanonicalResponse,
    ReasoningTransport, ResponsePart, ToolKind, TransformError,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(in crate::gateway::transform::stream) struct ChatToResponses {
    source: ChatSource,
    started: bool,
    text: Option<TextOutput>,
    calls: BTreeMap<u64, ResponseCall>,
    reasoning_transport: Option<ReasoningTransport>,
    pending_reasoning: String,
    reasoning_parts: BTreeMap<u64, ResponsePart>,
    next_output_index: u64,
    completed: bool,
}

impl ChatToResponses {
    pub(in crate::gateway::transform::stream) fn new(
        reasoning_transport: Option<ReasoningTransport>,
    ) -> Self {
        Self {
            source: ChatSource::default(),
            started: false,
            text: None,
            calls: BTreeMap::new(),
            reasoning_transport,
            pending_reasoning: String::new(),
            reasoning_parts: BTreeMap::new(),
            next_output_index: 0,
            completed: false,
        }
    }

    pub(in crate::gateway::transform::stream) fn on_frame(
        &mut self,
        frame: Frame,
    ) -> Result<Vec<u8>, TransformError> {
        let update = parse_chat_frame(&frame)?;
        self.source.apply(&update)?;
        let mut output = Vec::new();
        self.start(&mut output)?;
        for reasoning in update.reasoning {
            self.pending_reasoning.push_str(&reasoning);
        }
        for text in update.text {
            if !text.is_empty() {
                self.flush_reasoning(&mut output)?;
                self.push_text(&mut output, &text)?;
            }
        }
        for call in update.calls {
            self.flush_reasoning(&mut output)?;
            self.push_call(&mut output, call)?;
        }
        if update.done {
            self.complete(&mut output)?;
        }
        Ok(output)
    }

    pub(in crate::gateway::transform::stream) fn finish(
        &mut self,
    ) -> Result<Vec<u8>, TransformError> {
        if self.completed {
            Ok(Vec::new())
        } else {
            Err(TransformError("Chat SSE 缺少 [DONE] 终止帧".to_string()))
        }
    }

    fn start(&mut self, output: &mut Vec<u8>) -> Result<(), TransformError> {
        if self.started {
            return Ok(());
        }
        let Ok((id, model)) = self.source.identity() else {
            return Ok(());
        };
        append_event(
            output,
            "response.created",
            json!({
                "type": "response.created",
                "response": {
                    "id": id,
                    "object": "response",
                    "status": "in_progress",
                    "model": model,
                    "output": [],
                },
            }),
        );
        self.started = true;
        Ok(())
    }

    fn push_text(&mut self, output: &mut Vec<u8>, delta: &str) -> Result<(), TransformError> {
        self.start(output)?;
        if !self.started {
            return Err(TransformError(
                "Chat SSE 在 id 和 model 前发送文本".to_string(),
            ));
        }
        if self.text.is_none() {
            let (id, _) = self.source.identity()?;
            let index = self.next_output_index;
            self.next_output_index += 1;
            append_event(
                output,
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added",
                    "output_index": index,
                    "item": {
                        "type": "message",
                        "id": format!("msg_{id}"),
                        "status": "in_progress",
                        "role": "assistant",
                        "content": [],
                    },
                }),
            );
            append_event(
                output,
                "response.content_part.added",
                json!({
                    "type": "response.content_part.added",
                    "output_index": index,
                    "content_index": 0,
                    "part": { "type": "output_text", "text": "", "annotations": [] },
                }),
            );
            self.text = Some(TextOutput {
                index,
                text: String::new(),
            });
        }
        let text = self.text.as_mut().expect("created above");
        text.text.push_str(delta);
        append_event(
            output,
            "response.output_text.delta",
            json!({
                "type": "response.output_text.delta",
                "output_index": text.index,
                "content_index": 0,
                "delta": delta,
            }),
        );
        Ok(())
    }

    fn flush_reasoning(&mut self, output: &mut Vec<u8>) -> Result<(), TransformError> {
        if self.pending_reasoning.is_empty() {
            return Ok(());
        }
        self.start(output)?;
        if !self.started {
            return Err(TransformError(
                "Chat SSE 在 id 和 model 前发送推理内容".to_string(),
            ));
        }
        let transport = self
            .reasoning_transport
            .as_ref()
            .ok_or_else(|| TransformError("当前转换缺少本机推理续接通道".to_string()))?;
        let reasoning = transport.from_chat_content(std::mem::take(&mut self.pending_reasoning))?;
        let (id, _) = self.source.identity()?;
        let output_index = self.next_output_index;
        self.next_output_index += 1;
        let item = responses_reasoning_item(&id, output_index, &reasoning);
        append_event(
            output,
            "response.output_item.added",
            json!({
                "type": "response.output_item.added",
                "output_index": output_index,
                "item": item,
            }),
        );
        append_event(
            output,
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "output_index": output_index,
                "item": item,
            }),
        );
        self.reasoning_parts
            .insert(output_index, ResponsePart::Reasoning(reasoning));
        Ok(())
    }

    fn push_call(&mut self, output: &mut Vec<u8>, delta: CallDelta) -> Result<(), TransformError> {
        {
            let call = self.calls.entry(delta.index).or_default();
            merge_identity(&mut call.id, delta.id.as_deref(), "Chat tool_call.id")?;
            merge_identity(
                &mut call.name,
                delta.name.as_deref(),
                "Chat tool_call.function.name",
            )?;
            if let Some(arguments) = delta.arguments {
                call.arguments.push_str(&arguments);
            }
        }
        self.start_call(output, delta.index)?;
        let (output_index, kind, arguments) = {
            let call = self.calls.get_mut(&delta.index).expect("entry exists");
            let Some(output_index) = call.output_index else {
                return Ok(());
            };
            if call.sent_arguments == call.arguments.len() {
                return Ok(());
            }
            let arguments = call.arguments[call.sent_arguments..].to_string();
            call.sent_arguments = call.arguments.len();
            (output_index, call.kind, arguments)
        };
        if kind == ToolKind::Function {
            append_event(
                output,
                "response.function_call_arguments.delta",
                json!({
                    "type": "response.function_call_arguments.delta", "output_index": output_index, "delta": arguments,
                }),
            );
        }
        Ok(())
    }

    fn start_call(
        &mut self,
        output: &mut Vec<u8>,
        source_index: u64,
    ) -> Result<(), TransformError> {
        self.start(output)?;
        if !self.started {
            return Err(TransformError(
                "Chat SSE 在 id 和 model 前发送工具调用".to_string(),
            ));
        }
        let details = self.calls.get(&source_index).and_then(|call| {
            (call.output_index.is_none())
                .then(|| call.id.clone().zip(call.name.clone()))
                .flatten()
        });
        let Some((id, encoded_name)) = details else {
            return Ok(());
        };
        let (namespace, name, kind) = parse_target_name(&encoded_name)?;
        let output_index = self.next_output_index;
        self.next_output_index += 1;
        let call = self.calls.get_mut(&source_index).expect("entry exists");
        call.output_index = Some(output_index);
        call.rendered_name = Some(name.clone());
        call.namespace = namespace.clone();
        call.kind = kind;
        append_event(
            output,
            "response.output_item.added",
            json!({
                "type": "response.output_item.added",
                "output_index": output_index,
                "item": responses_tool_call_item(&id, &name, namespace.as_deref(), kind, "", "in_progress")?,
            }),
        );
        Ok(())
    }

    fn complete(&mut self, output: &mut Vec<u8>) -> Result<(), TransformError> {
        if self.completed {
            return Err(TransformError("Chat SSE 重复 [DONE] 终止帧".to_string()));
        }
        self.start(output)?;
        if !self.started {
            return Err(TransformError(
                "Chat SSE 在缺少 id 或 model 时结束".to_string(),
            ));
        }
        self.flush_reasoning(output)?;
        let (id, model) = self.source.identity()?;
        let mut parts = std::mem::take(&mut self.reasoning_parts);
        if let Some(text) = &self.text {
            append_event(
                output,
                "response.content_part.done",
                json!({
                    "type": "response.content_part.done",
                    "output_index": text.index,
                    "content_index": 0,
                    "part": { "type": "output_text", "text": text.text, "annotations": [] },
                }),
            );
            append_event(
                output,
                "response.output_item.done",
                json!({
                    "type": "response.output_item.done",
                    "output_index": text.index,
                    "item": {
                        "type": "message",
                        "id": format!("msg_{id}"),
                        "status": "completed",
                        "role": "assistant",
                        "content": [{ "type": "output_text", "text": text.text, "annotations": [] }],
                    },
                }),
            );
            parts.insert(text.index, ResponsePart::Text(text.text.clone()));
        }
        for source_index in self.calls.keys().copied().collect::<Vec<_>>() {
            self.start_call(output, source_index)?;
            let (output_index, call_id, name, namespace, kind, arguments) = {
                let call = self.calls.get_mut(&source_index).expect("entry exists");
                let output_index = call
                    .output_index
                    .ok_or_else(|| TransformError("Chat SSE 工具调用缺少 id 或名称".to_string()))?;
                if kind_is_function(call.kind) && call.sent_arguments < call.arguments.len() {
                    let delta = call.arguments[call.sent_arguments..].to_string();
                    call.sent_arguments = call.arguments.len();
                    append_event(
                        output,
                        "response.function_call_arguments.delta",
                        json!({
                            "type": "response.function_call_arguments.delta",
                            "output_index": output_index,
                            "delta": delta,
                        }),
                    );
                }
                (
                    output_index,
                    call.id.clone().expect("checked by start"),
                    call.rendered_name.clone().expect("set by start"),
                    call.namespace.clone(),
                    call.kind,
                    call.arguments.clone(),
                )
            };
            let input = parse_call_input(kind, &arguments)?;
            if kind == ToolKind::Custom {
                let input = input.as_str().expect("custom input parser returns string");
                if !input.is_empty() {
                    append_event(
                        output,
                        "response.custom_tool_call_input.delta",
                        json!({
                            "type":"response.custom_tool_call_input.delta", "output_index":output_index, "delta":input,
                        }),
                    );
                }
                append_event(
                    output,
                    "response.custom_tool_call_input.done",
                    json!({
                        "type":"response.custom_tool_call_input.done", "output_index":output_index, "input":input,
                    }),
                );
            }
            append_event(
                output,
                "response.output_item.done",
                json!({
                    "type": "response.output_item.done",
                    "output_index": output_index,
                    "item": responses_tool_call_item(
                        &call_id,
                        &name,
                        namespace.as_deref(),
                        kind,
                        &arguments,
                        "completed",
                    )?,
                }),
            );
            parts.insert(
                output_index,
                ResponsePart::ToolCall {
                    id: call_id,
                    name,
                    namespace,
                    kind,
                    input,
                },
            );
        }
        let response = CanonicalResponse {
            id,
            model,
            content: parts.into_values().collect(),
            stop: self.source.stop(),
            usage: self.source.usage.clone(),
        };
        append_event(
            output,
            "response.completed",
            json!({ "type": "response.completed", "response": responses_complete(&response)? }),
        );
        self.completed = true;
        Ok(())
    }
}

fn kind_is_function(kind: ToolKind) -> bool {
    matches!(kind, ToolKind::Function)
}

fn parse_call_input(kind: ToolKind, arguments: &str) -> Result<Value, TransformError> {
    let input: Value = serde_json::from_str(arguments)
        .map_err(|_| TransformError("Chat SSE 工具参数不是完整 JSON".to_string()))?;
    if matches!(kind, ToolKind::Function | ToolKind::ToolSearch) {
        return Ok(input);
    }
    let input = input
        .as_object()
        .filter(|value| value.len() == 1)
        .and_then(|value| value.get("input"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            TransformError("Chat SSE custom 工具参数必须是唯一 input 字符串".to_string())
        })?;
    Ok(Value::String(input.to_string()))
}

fn responses_tool_call_item(
    id: &str,
    name: &str,
    namespace: Option<&str>,
    kind: ToolKind,
    arguments: &str,
    status: &str,
) -> Result<Value, TransformError> {
    let mut item = Map::new();
    item.insert(
        "type".to_string(),
        Value::String(
            match kind {
                ToolKind::Function => "function_call",
                ToolKind::Custom => "custom_tool_call",
                ToolKind::ToolSearch => "tool_search_call",
            }
            .to_string(),
        ),
    );
    match kind {
        ToolKind::Function => {
            item.insert("id".to_string(), Value::String(format!("fc_{id}")));
        }
        ToolKind::Custom => {
            item.insert("id".to_string(), Value::String(format!("ctc_{id}")));
        }
        ToolKind::ToolSearch => {}
    }
    item.insert("call_id".to_string(), Value::String(id.to_string()));
    if kind != ToolKind::ToolSearch {
        item.insert("name".to_string(), Value::String(name.to_string()));
    }
    if let Some(namespace) = namespace {
        item.insert(
            "namespace".to_string(),
            Value::String(namespace.to_string()),
        );
    }
    match kind {
        ToolKind::Function => {
            item.insert(
                "arguments".to_string(),
                Value::String(arguments.to_string()),
            );
        }
        ToolKind::Custom => {
            let input = if arguments.is_empty() {
                Value::String(String::new())
            } else {
                parse_call_input(kind, arguments)?
            };
            item.insert("input".to_string(), input);
        }
        ToolKind::ToolSearch => {
            let arguments = if arguments.is_empty() {
                json!({})
            } else {
                parse_call_input(kind, arguments)?
            };
            item.insert("execution".to_string(), Value::String("client".to_string()));
            item.insert("arguments".to_string(), arguments);
        }
    }
    item.insert("status".to_string(), Value::String(status.to_string()));
    Ok(Value::Object(item))
}
