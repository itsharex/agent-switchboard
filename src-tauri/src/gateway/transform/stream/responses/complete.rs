//! Completion, flushing, and final-part emission for the streamed conversion.

use super::*;

impl ResponsesToAnthropic {
    pub(super) fn content_part_done(&self, frame: &Frame) -> Result<(), TransformError> {
        let value = json_data(frame, "response.content_part.done")?;
        let map = object(&value, "response.content_part.done")?;
        allowed(
            map,
            &[
                "type",
                "sequence_number",
                "output_index",
                "content_index",
                "part",
            ],
            "response.content_part.done",
        )?;
        if required_string(map, "type", "response.content_part.done")?
            != "response.content_part.done"
            || required_index(map, "content_index")? != 0
        {
            return Err(TransformError(
                "response.content_part.done 字段无效".to_string(),
            ));
        }
        let output_index = required_index(map, "output_index")?;
        if !matches!(self.items.get(&output_index), Some(Item::Text { .. })) {
            return Err(TransformError(
                "Responses content part done 找不到文本 output item".to_string(),
            ));
        }
        Ok(())
    }

    pub(super) fn item_done(&self, frame: &Frame) -> Result<(), TransformError> {
        let value = json_data(frame, "response.output_item.done")?;
        let map = object(&value, "response.output_item.done")?;
        allowed(
            map,
            &["type", "sequence_number", "output_index", "item"],
            "response.output_item.done",
        )?;
        if required_string(map, "type", "response.output_item.done")? != "response.output_item.done"
        {
            return Err(TransformError(
                "response.output_item.done.type 无效".to_string(),
            ));
        }
        let output_index = required_index(map, "output_index")?;
        if !self.items.contains_key(&output_index) {
            return Err(TransformError(
                "Responses output item done 找不到起始 item".to_string(),
            ));
        }
        Ok(())
    }

    pub(super) fn complete(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "response.completed")?;
        let map = object(&value, "response.completed")?;
        allowed(
            map,
            &["type", "sequence_number", "response"],
            "response.completed",
        )?;
        if required_string(map, "type", "response.completed")? != "response.completed" {
            return Err(TransformError("response.completed.type 无效".to_string()));
        }
        let final_response = map
            .get("response")
            .ok_or_else(|| TransformError("response.completed 缺少 response".to_string()))?;
        let canonical = parse_responses_complete(final_response)?;
        merge(&mut self.id, &canonical.id, "Responses SSE id")?;
        merge(&mut self.model, &canonical.model, "Responses SSE model")?;
        self.start(output)?;
        self.flush_final(&canonical.content, output)?;
        append_event(
            output,
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": { "stop_reason": anthropic_stop_reason(canonical.stop), "stop_sequence": null },
                "usage": { "output_tokens": canonical.usage.output_tokens.unwrap_or(0) },
            }),
        );
        append_event(output, "message_stop", json!({ "type": "message_stop" }));
        self.completed = true;
        Ok(())
    }

    pub(super) fn flush_final(
        &mut self,
        expected: &[ResponsePart],
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let existing_indices = self.items.keys().copied().collect::<Vec<_>>();
        if existing_indices.len() > expected.len() {
            return Err(TransformError(
                "Responses 完整响应少于已发送的流式输出项".to_string(),
            ));
        }
        for (position, output_index) in existing_indices.iter().enumerate() {
            let expected = expected
                .get(position)
                .ok_or_else(|| TransformError("Responses 完整响应缺少流式输出项".to_string()))?;
            self.finish_existing(*output_index, expected, output)?;
        }
        for part in expected.iter().skip(existing_indices.len()) {
            let content_index = self.next_content_index;
            self.next_content_index += 1;
            emit_complete_part(output, content_index, part)?;
        }
        Ok(())
    }

    pub(super) fn finish_existing(
        &mut self,
        output_index: u64,
        expected: &ResponsePart,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        match (self.items.get_mut(&output_index), expected) {
            (
                Some(Item::Text {
                    content_index,
                    text,
                }),
                ResponsePart::Text(final_text),
            ) => {
                let suffix = append_suffix(text, final_text, "Responses 完整文本")?;
                if !suffix.is_empty() {
                    append_event(
                        output,
                        "content_block_delta",
                        json!({
                            "type": "content_block_delta",
                            "index": *content_index,
                            "delta": { "type": "text_delta", "text": suffix },
                        }),
                    );
                }
                append_event(
                    output,
                    "content_block_stop",
                    json!({ "type": "content_block_stop", "index": *content_index }),
                );
                Ok(())
            }
            (
                Some(Item::Tool {
                    content_index,
                    id,
                    name,
                    arguments,
                }),
                ResponsePart::ToolCall {
                    id: expected_id,
                    name: expected_name,
                    namespace: expected_namespace,
                    input,
                },
            ) => {
                let expected_name = render_target_name(
                    UpstreamProtocol::AnthropicMessages,
                    expected_namespace.as_deref(),
                    expected_name,
                )?;
                if id != expected_id || name != &expected_name {
                    return Err(TransformError(
                        "Responses 完整工具调用与流式输出不一致".to_string(),
                    ));
                }
                let final_arguments = serde_json::to_string(input)
                    .map_err(|_| TransformError("无法编码 Responses 工具参数".to_string()))?;
                let suffix = argument_suffix(arguments, &final_arguments)?;
                if !suffix.is_empty() {
                    append_event(
                        output,
                        "content_block_delta",
                        json!({
                            "type": "content_block_delta",
                            "index": *content_index,
                            "delta": { "type": "input_json_delta", "partial_json": suffix },
                        }),
                    );
                }
                append_event(
                    output,
                    "content_block_stop",
                    json!({ "type": "content_block_stop", "index": *content_index }),
                );
                Ok(())
            }
            _ => Err(TransformError(
                "Responses 完整输出类型与流式输出不一致".to_string(),
            )),
        }
    }

    pub(super) fn start(&mut self, output: &mut Vec<u8>) -> Result<(), TransformError> {
        if self.started {
            return Ok(());
        }
        let id = self
            .id
            .as_deref()
            .ok_or_else(|| TransformError("Responses SSE 缺少 id".to_string()))?;
        let model = self
            .model
            .as_deref()
            .ok_or_else(|| TransformError("Responses SSE 缺少 model".to_string()))?;
        append_event(
            output,
            "message_start",
            json!({
                "type": "message_start",
                "message": {
                    "id": id,
                    "type": "message",
                    "role": "assistant",
                    "model": model,
                    "content": [],
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": { "input_tokens": 0, "output_tokens": 0 },
                },
            }),
        );
        self.started = true;
        Ok(())
    }

    pub(super) fn require_started(&self) -> Result<(), TransformError> {
        if self.started {
            Ok(())
        } else {
            Err(TransformError(
                "Responses SSE 在 response.created 前发送内容".to_string(),
            ))
        }
    }
}

pub(super) fn emit_complete_part(
    output: &mut Vec<u8>,
    content_index: u64,
    part: &ResponsePart,
) -> Result<(), TransformError> {
    match part {
        ResponsePart::Text(text) => {
            append_event(
                output,
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": content_index,
                    "content_block": { "type": "text", "text": "" },
                }),
            );
            if !text.is_empty() {
                append_event(
                    output,
                    "content_block_delta",
                    json!({
                        "type": "content_block_delta",
                        "index": content_index,
                        "delta": { "type": "text_delta", "text": text },
                    }),
                );
            }
        }
        ResponsePart::Reasoning(reasoning) => {
            append_event(
                output,
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": content_index,
                    "content_block": {
                        "type": "redacted_thinking",
                        "data": reasoning.continuation,
                    },
                }),
            );
        }
        ResponsePart::ToolCall {
            id,
            name,
            namespace,
            input,
        } => {
            let arguments = serde_json::to_string(input)
                .map_err(|_| TransformError("无法编码 Responses 工具参数".to_string()))?;
            append_event(
                output,
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": content_index,
                    "content_block": {
                        "type": "tool_use",
                        "id": id,
                        "name": render_target_name(
                            UpstreamProtocol::AnthropicMessages,
                            namespace.as_deref(),
                            name,
                        )?,
                        "input": {},
                    },
                }),
            );
            append_event(
                output,
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": content_index,
                    "delta": { "type": "input_json_delta", "partial_json": arguments },
                }),
            );
        }
    }
    append_event(
        output,
        "content_block_stop",
        json!({ "type": "content_block_stop", "index": content_index }),
    );
    Ok(())
}

pub(super) fn append_suffix(
    current: &mut String,
    final_value: &str,
    context: &str,
) -> Result<String, TransformError> {
    let Some(suffix) = final_value.strip_prefix(current.as_str()) else {
        return Err(TransformError(format!("{context} 与已发送流式内容不一致")));
    };
    let suffix = suffix.to_string();
    current.push_str(&suffix);
    Ok(suffix)
}

pub(super) fn argument_suffix(
    current: &mut String,
    final_value: &str,
) -> Result<String, TransformError> {
    if let Some(suffix) = final_value.strip_prefix(current.as_str()) {
        let suffix = suffix.to_string();
        current.push_str(&suffix);
        return Ok(suffix);
    }
    let current_value: Value = serde_json::from_str(current)
        .map_err(|_| TransformError("Responses 流式工具参数与完整参数不一致".to_string()))?;
    let final_value_json: Value = serde_json::from_str(final_value)
        .map_err(|_| TransformError("Responses 完整工具参数不是 JSON".to_string()))?;
    if current_value == final_value_json {
        Ok(String::new())
    } else {
        Err(TransformError(
            "Responses 流式工具参数与完整参数不一致".to_string(),
        ))
    }
}

pub(super) fn merge(
    destination: &mut Option<String>,
    incoming: &str,
    context: &str,
) -> Result<(), TransformError> {
    match destination {
        Some(current) if current != incoming => {
            Err(TransformError(format!("{context} 在流中变化")))
        }
        Some(_) => Ok(()),
        None => {
            *destination = Some(incoming.to_string());
            Ok(())
        }
    }
}
