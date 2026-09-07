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
            Block::Text { stopped, .. } | Block::Tool { stopped, .. } => !stopped,
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
                    arguments,
                    ..
                } => {
                    let input = serde_json::from_str(arguments).map_err(|_| {
                        TransformError("Anthropic SSE 工具参数不是完整 JSON".to_string())
                    })?;
                    append_event(
                        output,
                        "response.output_item.done",
                        json!({
                            "type": "response.output_item.done",
                            "output_index": index,
                            "item": responses_function_call_item(
                                call_id,
                                name,
                                namespace.as_deref(),
                                arguments,
                                "completed",
                            ),
                        }),
                    );
                    parts.insert(
                        *index,
                        ResponsePart::ToolCall {
                            id: call_id.clone(),
                            name: name.clone(),
                            namespace: namespace.clone(),
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

pub(super) fn responses_function_call_item(
    id: &str,
    name: &str,
    namespace: Option<&str>,
    arguments: &str,
    status: &str,
) -> Value {
    let mut item = Map::new();
    item.insert(
        "type".to_string(),
        Value::String("function_call".to_string()),
    );
    item.insert("id".to_string(), Value::String(format!("fc_{id}")));
    item.insert("call_id".to_string(), Value::String(id.to_string()));
    item.insert("name".to_string(), Value::String(name.to_string()));
    if let Some(namespace) = namespace {
        item.insert(
            "namespace".to_string(),
            Value::String(namespace.to_string()),
        );
    }
    item.insert(
        "arguments".to_string(),
        Value::String(arguments.to_string()),
    );
    item.insert("status".to_string(), Value::String(status.to_string()));
    Value::Object(item)
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
