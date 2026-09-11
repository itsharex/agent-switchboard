//! Completion handling and shared item construction for the conversion.

use super::*;

impl AnthropicToResponses {
    pub(super) fn complete(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        self.require_started()?;
        let value = json_data(frame, "message_stop")?;
        let map = object(&value, "message_stop")?;
        allowed(map, &["type"], "message_stop")?;
        if required_string(map, "type", "message_stop")? != "message_stop" {
            return Err(TransformError("message_stop.type 无效".to_string()));
        }
        if self.blocks.values().any(|block| match block {
            Block::Text { stopped, .. }
            | Block::Thinking { stopped, .. }
            | Block::Redacted { stopped, .. }
            | Block::Tool { stopped, .. } => !stopped,
        }) {
            return Err(TransformError(
                "message_stop 前存在未结束的 content block".to_string(),
            ));
        }
        let id = self.id.clone().expect("started");
        let model = self.model.clone().expect("started");
        let mut parts = BTreeMap::new();
        for (index, block) in &self.blocks {
            match block {
                Block::Thinking { reasoning, .. } | Block::Redacted { reasoning, .. } => {
                    if let Some(reasoning) = reasoning {
                        parts.insert(*index, ResponsePart::Reasoning(reasoning.clone()));
                    }
                }
                Block::Text { text, .. } => {
                    append_event(
                        output,
                        "response.content_part.done",
                        json!({
                            "type": "response.content_part.done",
                            "output_index": index,
                            "content_index": 0,
                            "part": { "type": "output_text", "text": text, "annotations": [] },
                        }),
                    );
                    append_event(
                        output,
                        "response.output_item.done",
                        json!({
                            "type": "response.output_item.done",
                            "output_index": index,
                            "item": {
                                "type": "message",
                                "id": format!("msg_{id}_{index}"),
                                "status": "completed",
                                "role": "assistant",
                                "content": [{ "type": "output_text", "text": text, "annotations": [] }],
                            },
                        }),
                    );
                    parts.insert(*index, ResponsePart::Text(text.clone()));
                }
                Block::Tool {
                    id: call_id,
                    name,
                    namespace,
                    kind,
                    arguments,
                    ..
                } => {
                    let input = parse_tool_input(*kind, arguments)?;
                    if *kind == ToolKind::Custom {
                        let input = input.as_str().expect("custom input parser returns string");
                        if !input.is_empty() {
                            append_event(
                                output,
                                "response.custom_tool_call_input.delta",
                                json!({
                                    "type":"response.custom_tool_call_input.delta", "output_index":index, "delta":input,
                                }),
                            );
                        }
                        append_event(
                            output,
                            "response.custom_tool_call_input.done",
                            json!({
                                "type":"response.custom_tool_call_input.done", "output_index":index, "input":input,
                            }),
                        );
                    }
                    append_event(
                        output,
                        "response.output_item.done",
                        json!({
                            "type": "response.output_item.done",
                            "output_index": index,
                            "item": responses_tool_call_item(
                                call_id,
                                name,
                                namespace.as_deref(),
                                *kind,
                                arguments,
                                "completed",
                            )?,
                        }),
                    );
                    parts.insert(
                        *index,
                        ResponsePart::ToolCall {
                            id: call_id.clone(),
                            name: name.clone(),
                            namespace: namespace.clone(),
                            kind: *kind,
                            input,
                        },
                    );
                }
            }
        }
        let response = CanonicalResponse {
            id,
            model,
            content: parts.into_values().collect(),
            stop: match self.stop {
                StopReasonState::Unset => StopReason::EndTurn,
                StopReasonState::Value(reason) => reason,
            },
            usage: self.usage.clone(),
        };
        append_event(
            output,
            "response.completed",
            json!({ "type": "response.completed", "response": responses_complete(&response)? }),
        );
        self.completed = true;
        Ok(())
    }

    pub(super) fn require_started(&self) -> Result<(), TransformError> {
        if self.started {
            Ok(())
        } else {
            Err(TransformError(
                "Anthropic SSE 在 message_start 前发送内容".to_string(),
            ))
        }
    }
}

fn parse_tool_input(kind: ToolKind, arguments: &str) -> Result<Value, TransformError> {
    let input: Value = serde_json::from_str(arguments)
        .map_err(|_| TransformError("Anthropic SSE 工具参数不是完整 JSON".to_string()))?;
    if matches!(kind, ToolKind::Function | ToolKind::ToolSearch) {
        return Ok(input);
    }
    input
        .as_object()
        .filter(|value| value.len() == 1)
        .and_then(|value| value.get("input"))
        .and_then(Value::as_str)
        .map(|input| Value::String(input.to_string()))
        .ok_or_else(|| {
            TransformError("Anthropic SSE custom 工具参数必须是唯一 input 字符串".to_string())
        })
}

/// Releases one sealed reasoning trace to the client exactly once, before the
/// assistant message that followed it upstream.
pub(super) fn release_reasoning(
    output: &mut Vec<u8>,
    id: Option<&str>,
    output_index: u64,
    reasoning: &Reasoning,
) {
    let item = responses_reasoning_item(
        id.expect("a block requires message_start"),
        output_index,
        reasoning,
    );
    for event in ["response.output_item.added", "response.output_item.done"] {
        append_event(
            output,
            event,
            json!({
                "type": event,
                "output_index": output_index,
                "item": item,
            }),
        );
    }
}

pub(super) fn responses_tool_call_item(
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
                parse_tool_input(kind, arguments)?
            };
            item.insert("input".to_string(), input);
        }
        ToolKind::ToolSearch => {
            let arguments = if arguments.is_empty() {
                json!({})
            } else {
                parse_tool_input(kind, arguments)?
            };
            item.insert("execution".to_string(), Value::String("client".to_string()));
            item.insert("arguments".to_string(), arguments);
        }
    }
    item.insert("status".to_string(), Value::String(status.to_string()));
    Ok(Value::Object(item))
}

pub(super) fn validate_ping(frame: &Frame) -> Result<(), TransformError> {
    let value = json_data(frame, "ping")?;
    let map = object(&value, "ping")?;
    allowed(map, &["type"], "ping")?;
    if required_string(map, "type", "ping")? == "ping" {
        Ok(())
    } else {
        Err(TransformError("ping.type 无效".to_string()))
    }
}
