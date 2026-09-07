//! Streaming block-content handlers for the Anthropic-to-Responses conversion.

use super::*;

impl AnthropicToResponses {
    pub(super) fn message_start(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        if self.started {
            return Err(TransformError(
                "Anthropic SSE 重复 message_start".to_string(),
            ));
        }
        let value = json_data(frame, "message_start")?;
        let map = object(&value, "message_start")?;
        allowed(map, &["type", "message"], "message_start")?;
        if required_string(map, "type", "message_start")? != "message_start" {
            return Err(TransformError("message_start.type 无效".to_string()));
        }
        let message = object(
            map.get("message")
                .ok_or_else(|| TransformError("message_start 缺少 message".to_string()))?,
            "message_start.message",
        )?;
        allowed(
            message,
            &[
                "id",
                "type",
                "role",
                "model",
                "content",
                "stop_reason",
                "stop_sequence",
                "usage",
            ],
            "message_start.message",
        )?;
        if required_string(message, "type", "message_start.message")? != "message"
            || required_string(message, "role", "message_start.message")? != "assistant"
        {
            return Err(TransformError(
                "message_start.message 不是 assistant message".to_string(),
            ));
        }
        if !message
            .get("content")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        {
            return Err(TransformError(
                "message_start.message.content 必须为空数组".to_string(),
            ));
        }
        if message
            .get("stop_reason")
            .is_some_and(|value| !value.is_null())
            || message
                .get("stop_sequence")
                .is_some_and(|value| !value.is_null())
        {
            return Err(TransformError("message_start 不应包含终止原因".to_string()));
        }
        self.id = Some(required_string(message, "id", "message_start.message")?);
        self.model = Some(required_string(message, "model", "message_start.message")?);
        self.usage = parse_usage(
            message
                .get("usage")
                .ok_or_else(|| TransformError("message_start 缺少 usage".to_string()))?,
        )?;
        let id = self.id.as_deref().expect("set above");
        let model = self.model.as_deref().expect("set above");
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

    pub(super) fn block_start(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        self.require_started()?;
        let value = json_data(frame, "content_block_start")?;
        let map = object(&value, "content_block_start")?;
        allowed(
            map,
            &["type", "index", "content_block"],
            "content_block_start",
        )?;
        if required_string(map, "type", "content_block_start")? != "content_block_start" {
            return Err(TransformError("content_block_start.type 无效".to_string()));
        }
        let index = required_index(map, "content_block_start")?;
        if self.blocks.contains_key(&index) {
            return Err(TransformError(
                "Anthropic SSE 重复 content block 索引".to_string(),
            ));
        }
        let block = object(
            map.get("content_block").ok_or_else(|| {
                TransformError("content_block_start 缺少 content_block".to_string())
            })?,
            "content_block_start.content_block",
        )?;
        let kind = required_string(block, "type", "content_block")?;
        match kind.as_str() {
            "text" => {
                allowed(block, &["type", "text"], "Anthropic text block")?;
                // Anthropic starts ordinary text blocks with an empty text
                // value, then sends the first token in `text_delta`.
                let text = block
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| {
                        TransformError("Anthropic text block.text 必须是字符串".to_string())
                    })?;
                let id = self.id.as_deref().expect("started");
                append_event(
                    output,
                    "response.output_item.added",
                    json!({
                        "type": "response.output_item.added",
                        "output_index": index,
                        "item": {
                            "type": "message",
                            "id": format!("msg_{id}_{index}"),
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
                if !text.is_empty() {
                    append_event(
                        output,
                        "response.output_text.delta",
                        json!({
                            "type": "response.output_text.delta",
                            "output_index": index,
                            "content_index": 0,
                            "delta": text,
                        }),
                    );
                }
                self.blocks.insert(
                    index,
                    Block::Text {
                        text,
                        stopped: false,
                    },
                );
            }
            "tool_use" => {
                allowed(
                    block,
                    &["type", "id", "name", "input"],
                    "Anthropic tool block",
                )?;
                let input = block
                    .get("input")
                    .and_then(Value::as_object)
                    .ok_or_else(|| {
                        TransformError("Anthropic tool block.input 必须是对象".to_string())
                    })?;
                if !input.is_empty() {
                    return Err(TransformError(
                        "Anthropic 流式 tool_use 初始 input 必须为空对象".to_string(),
                    ));
                }
                let id = required_string(block, "id", "Anthropic tool block")?;
                let encoded_name = required_string(block, "name", "Anthropic tool block")?;
                let (namespace, name) = parse_target_name(&encoded_name)?;
                append_event(
                    output,
                    "response.output_item.added",
                    json!({
                        "type": "response.output_item.added",
                        "output_index": index,
                        "item": responses_function_call_item(
                            &id,
                            &name,
                            namespace.as_deref(),
                            "",
                            "in_progress",
                        ),
                    }),
                );
                self.blocks.insert(
                    index,
                    Block::Tool {
                        id,
                        name,
                        namespace,
                        arguments: String::new(),
                        stopped: false,
                    },
                );
            }
            other => {
                return Err(TransformError(format!(
                    "Anthropic SSE content block {other} 不支持转换"
                )))
            }
        }
        Ok(())
    }

    pub(super) fn block_delta(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        self.require_started()?;
        let value = json_data(frame, "content_block_delta")?;
        let map = object(&value, "content_block_delta")?;
        allowed(map, &["type", "index", "delta"], "content_block_delta")?;
        if required_string(map, "type", "content_block_delta")? != "content_block_delta" {
            return Err(TransformError("content_block_delta.type 无效".to_string()));
        }
        let index = required_index(map, "content_block_delta")?;
        let delta = object(
            map.get("delta")
                .ok_or_else(|| TransformError("content_block_delta 缺少 delta".to_string()))?,
            "content_block_delta.delta",
        )?;
        let kind = required_string(delta, "type", "content_block_delta.delta")?;
        match (self.blocks.get_mut(&index), kind.as_str()) {
            (Some(Block::Text { text, stopped }), "text_delta") => {
                allowed(delta, &["type", "text"], "Anthropic text delta")?;
                if *stopped {
                    return Err(TransformError(
                        "文本 block 已结束后继续发送 delta".to_string(),
                    ));
                }
                let value = required_string(delta, "text", "Anthropic text delta")?;
                text.push_str(&value);
                if !value.is_empty() {
                    append_event(
                        output,
                        "response.output_text.delta",
                        json!({
                            "type": "response.output_text.delta",
                            "output_index": index,
                            "content_index": 0,
                            "delta": value,
                        }),
                    );
                }
            }
            (
                Some(Block::Tool {
                    arguments, stopped, ..
                }),
                "input_json_delta",
            ) => {
                allowed(delta, &["type", "partial_json"], "Anthropic tool delta")?;
                if *stopped {
                    return Err(TransformError(
                        "工具 block 已结束后继续发送 delta".to_string(),
                    ));
                }
                let value = required_string(delta, "partial_json", "Anthropic tool delta")?;
                arguments.push_str(&value);
                if !value.is_empty() {
                    append_event(
                        output,
                        "response.function_call_arguments.delta",
                        json!({
                            "type": "response.function_call_arguments.delta",
                            "output_index": index,
                            "delta": value,
                        }),
                    );
                }
            }
            (Some(_), _) => {
                return Err(TransformError(
                    "Anthropic SSE content block delta 与起始类型不匹配".to_string(),
                ))
            }
            (None, _) => {
                return Err(TransformError(
                    "content_block_delta 找不到起始 block".to_string(),
                ))
            }
        }
        Ok(())
    }

    pub(super) fn block_stop(&mut self, frame: &Frame) -> Result<(), TransformError> {
        let value = json_data(frame, "content_block_stop")?;
        let map = object(&value, "content_block_stop")?;
        allowed(map, &["type", "index"], "content_block_stop")?;
        if required_string(map, "type", "content_block_stop")? != "content_block_stop" {
            return Err(TransformError("content_block_stop.type 无效".to_string()));
        }
        let index = required_index(map, "content_block_stop")?;
        match self.blocks.get_mut(&index) {
            Some(Block::Text { stopped, .. } | Block::Tool { stopped, .. }) if !*stopped => {
                *stopped = true;
                Ok(())
            }
            Some(_) => Err(TransformError(
                "Anthropic SSE 重复 content_block_stop".to_string(),
            )),
            None => Err(TransformError(
                "content_block_stop 找不到起始 block".to_string(),
            )),
        }
    }

    pub(super) fn message_delta(&mut self, frame: &Frame) -> Result<(), TransformError> {
        self.require_started()?;
        let value = json_data(frame, "message_delta")?;
        let map = object(&value, "message_delta")?;
        allowed(map, &["type", "delta", "usage"], "message_delta")?;
        if required_string(map, "type", "message_delta")? != "message_delta" {
            return Err(TransformError("message_delta.type 无效".to_string()));
        }
        let delta = object(
            map.get("delta")
                .ok_or_else(|| TransformError("message_delta 缺少 delta".to_string()))?,
            "message_delta.delta",
        )?;
        allowed(
            delta,
            &["stop_reason", "stop_sequence"],
            "message_delta.delta",
        )?;
        if delta
            .get("stop_sequence")
            .is_some_and(|value| !value.is_null())
        {
            return Err(TransformError(
                "Anthropic stop_sequence 无法无损转换到 Responses".to_string(),
            ));
        }
        if let Some(reason) = delta.get("stop_reason") {
            if !reason.is_null() {
                self.stop =
                    StopReasonState::Value(parse_stop(reason.as_str().ok_or_else(|| {
                        TransformError("stop_reason 必须是字符串或 null".to_string())
                    })?)?);
            }
        }
        if let Some(usage) = map.get("usage") {
            let usage = parse_usage(usage)?;
            if usage.output_tokens.is_some() {
                self.usage.output_tokens = usage.output_tokens;
                self.usage.total_tokens = self
                    .usage
                    .input_tokens
                    .zip(self.usage.output_tokens)
                    .map(|(input, output)| input + output);
            }
        }
        Ok(())
    }
}
