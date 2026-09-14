//! Anthropic response envelopes and typed content blocks.

use super::*;
use crate::gateway::transform::usage;

pub(super) fn parse(
    value: &Value,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<CanonicalResponse, TransformError> {
    let map = object(value, "Anthropic 响应")?;
    allowed(
        map,
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
        "Anthropic 响应",
    )?;
    if string(map.get("role"), "role")? != "assistant" {
        return error("Anthropic 响应 role 不是 assistant");
    }
    let mut content = Vec::new();
    for value in array(
        map.get("content")
            .ok_or_else(|| TransformError("Anthropic 响应缺少 content".to_string()))?,
        "content",
    )? {
        parse_part(value, reasoning_transport, &mut content)?;
    }
    Ok(CanonicalResponse {
        id: string(map.get("id"), "id")?,
        model: string(map.get("model"), "model")?,
        content,
        stop: parse_anthropic_stop(map.get("stop_reason"))?,
        usage: usage::parse(UpstreamProtocol::AnthropicMessages, map.get("usage"))?,
    })
}

fn parse_part(
    value: &Value,
    reasoning_transport: Option<&ReasoningTransport>,
    content: &mut Vec<ResponsePart>,
) -> Result<(), TransformError> {
    let part = object(value, "Anthropic content")?;
    let kind = string(part.get("type"), "content.type")?;
    match kind.as_str() {
        "text" => parse_text(part, content)?,
        "tool_use" => parse_tool_use(part, content)?,
        "thinking" => parse_thinking(part, reasoning_transport, content)?,
        "redacted_thinking" => parse_redacted_thinking(part, reasoning_transport, content)?,
        other => return error(format!("Anthropic content.type {other} 不支持转换")),
    }
    Ok(())
}

fn parse_text(
    part: &Map<String, Value>,
    content: &mut Vec<ResponsePart>,
) -> Result<(), TransformError> {
    allowed(part, &["type", "text", "citations"], "Anthropic text")?;
    if part.get("citations").is_some() && !part.get("citations").is_some_and(Value::is_null) {
        return error("Anthropic citations 无法安全转换");
    }
    content.push(ResponsePart::Text(string(part.get("text"), "text")?));
    Ok(())
}

fn parse_tool_use(
    part: &Map<String, Value>,
    content: &mut Vec<ResponsePart>,
) -> Result<(), TransformError> {
    allowed(part, &["type", "id", "name", "input"], "Anthropic tool_use")?;
    content.push(ResponsePart::ToolCall {
        id: string(part.get("id"), "tool_use.id")?,
        name: string(part.get("name"), "tool_use.name")?,
        namespace: None,
        kind: ToolKind::Function,
        input: part
            .get("input")
            .cloned()
            .ok_or_else(|| TransformError("tool_use 缺少 input".to_string()))?,
    });
    Ok(())
}

fn parse_thinking(
    part: &Map<String, Value>,
    reasoning_transport: Option<&ReasoningTransport>,
    content: &mut Vec<ResponsePart>,
) -> Result<(), TransformError> {
    allowed(
        part,
        &["type", "thinking", "signature"],
        "Anthropic thinking",
    )?;
    // The upstream signature authenticates this exact trace, so it
    // is sealed with the text and replayed verbatim on the next
    // turn instead of being replaced by a local placeholder.
    let signature = match part.get("signature") {
        None | Some(Value::Null) => None,
        Some(value) => Some(string(Some(value), "thinking.signature")?),
    };
    let thinking = string(part.get("thinking"), "thinking")?;
    if !thinking.is_empty() {
        let transport = reasoning_transport
            .ok_or_else(|| TransformError("当前转换缺少本机推理续接通道".to_string()))?;
        content.push(ResponsePart::Reasoning(
            transport.from_anthropic_thinking(thinking, signature)?,
        ));
    }
    Ok(())
}

fn parse_redacted_thinking(
    part: &Map<String, Value>,
    reasoning_transport: Option<&ReasoningTransport>,
    content: &mut Vec<ResponsePart>,
) -> Result<(), TransformError> {
    allowed(part, &["type", "data"], "Anthropic redacted_thinking")?;
    let transport = reasoning_transport
        .ok_or_else(|| TransformError("当前转换缺少本机推理续接通道".to_string()))?;
    let data = string(part.get("data"), "redacted_thinking.data")?;
    // A payload this gateway issued is reopened; anything else is
    // the upstream's own opaque block and is sealed for replay.
    let reasoning = if ReasoningTransport::is_continuation(&data) {
        transport.from_continuation(data)?
    } else {
        transport.from_redacted(data)?
    };
    content.push(ResponsePart::Reasoning(reasoning));
    Ok(())
}
