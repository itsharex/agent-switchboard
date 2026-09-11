//! Content-part rendering shared by the request renderers.

use super::*;

/// Replays one reasoning trace to an Anthropic upstream in the only shape that
/// backend accepts: its own opaque block, or a thinking block carrying the text
/// and, when the upstream issued one, its signature.
fn replay_anthropic_reasoning(reasoning: &Reasoning) -> Result<Value, TransformError> {
    if let Some(redacted) = &reasoning.redacted {
        return Ok(json!({ "type": "redacted_thinking", "data": redacted }));
    }
    if reasoning.content.is_empty() {
        return error("该推理没有可发送给 Anthropic 上游的可读内容");
    }
    let mut block = json!({ "type": "thinking", "thinking": reasoning.content });
    if let Some(signature) = &reasoning.signature {
        block["signature"] = Value::String(signature.clone());
    }
    Ok(block)
}

pub(super) fn render_chat_content(parts: &[Part]) -> Result<Value, TransformError> {
    if parts.iter().all(|part| matches!(part, Part::Text(_))) {
        return Ok(Value::String(
            parts
                .iter()
                .filter_map(|part| match part {
                    Part::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        ));
    }
    let mut values = Vec::new();
    for part in parts {
        match part {
            Part::Text(text) => values.push(json!({ "type": "text", "text": text })),
            Part::Image(source) => values.push(json!({
                "type": "image_url",
                "image_url": { "url": image_url(source) },
            })),
            Part::ToolCall { .. } | Part::ToolResult { .. } => {
                return error("工具内容不能嵌入 Chat 文本内容数组");
            }
            Part::Reasoning(_) => return error("推理内容不能嵌入 Chat 文本内容数组"),
        }
    }
    Ok(Value::Array(values))
}

pub(super) fn render_anthropic_content(parts: &[Part]) -> Result<Value, TransformError> {
    let mut values = Vec::new();
    for part in parts {
        match part {
            Part::Text(text) => values.push(json!({ "type": "text", "text": text })),
            Part::Image(ImageSource::Data { media_type, data }) => values.push(json!({
                "type": "image",
                "source": { "type": "base64", "media_type": media_type, "data": data },
            })),
            Part::Image(ImageSource::Url(_)) => {
                return error("远程图片 URL 无法无损转换到 Anthropic Messages；请使用 base64 图片");
            }
            Part::ToolCall {
                id,
                name,
                namespace,
                kind,
                input,
            } => values.push(json!({
                "type": "tool_use",
                "id": id,
                "name": render_target_name(
                    UpstreamProtocol::AnthropicMessages,
                    namespace.as_deref(),
                    name,
                    *kind,
                )?,
                "input": if *kind == ToolKind::Custom { json!({ "input": input.as_str().ok_or_else(|| TransformError("custom 工具输入必须是字符串".to_string()))? }) } else { input.clone() },
            })),
            Part::ToolResult {
                id,
                content,
                is_error,
                ..
            } => values.push(json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": render_anthropic_content(content)?,
                "is_error": is_error,
            })),
            Part::Reasoning(reasoning) => values.push(replay_anthropic_reasoning(reasoning)?),
        }
    }
    Ok(Value::Array(values))
}

pub(super) fn render_responses_content(parts: &[Part]) -> Result<Value, TransformError> {
    let mut values = Vec::new();
    for part in parts {
        match part {
            Part::Text(text) => values.push(json!({ "type": "input_text", "text": text })),
            Part::Image(source) => values.push(json!({
                "type": "input_image",
                "image_url": image_url(source),
            })),
            Part::ToolCall { .. } | Part::ToolResult { .. } => {
                return error("工具内容不能嵌入 Responses message content");
            }
            Part::Reasoning(_) => return error("推理内容不能嵌入 Responses message content"),
        }
    }
    Ok(Value::Array(values))
}

pub(super) fn image_url(source: &ImageSource) -> String {
    match source {
        ImageSource::Data { media_type, data } => format!("data:{media_type};base64,{data}"),
        ImageSource::Url(url) => url.clone(),
    }
}

pub(super) fn text_only(parts: &[Part], context: &str) -> Result<String, TransformError> {
    let mut text = String::new();
    for part in parts {
        let Part::Text(value) = part else {
            return error(format!("{context} 只能包含文本"));
        };
        text.push_str(value);
    }
    Ok(text)
}
