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
