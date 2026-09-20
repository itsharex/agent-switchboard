//! Protocol-specific response renderers from the canonical response.

use super::*;

pub(super) fn render_chat(response: &CanonicalResponse) -> Result<Value, TransformError> {
    let text: String = response
        .content
        .iter()
        .filter_map(|part| match part {
            ResponsePart::Text(value) => Some(value.as_str()),
            ResponsePart::Reasoning(_) | ResponsePart::ToolCall { .. } => None,
        })
        .collect();
    let reasoning: String = response
        .content
        .iter()
        .filter_map(|part| match part {
            ResponsePart::Reasoning(value) => Some(value.content.as_str()),
            ResponsePart::Text(_) | ResponsePart::ToolCall { .. } => None,
        })
        .collect();
    let calls = render_chat_calls(response)?;
    let mut message = Map::new();
    message.insert("role".to_string(), Value::String("assistant".to_string()));
    message.insert(
        "content".to_string(),
        if text.is_empty() {
            Value::Null
        } else {
            Value::String(text)
        },
    );
    if !calls.is_empty() {
        message.insert("tool_calls".to_string(), Value::Array(calls));
    }
    if !reasoning.is_empty() {
        message.insert("reasoning_content".to_string(), Value::String(reasoning));
    }
    Ok(json!({
        "id": response.id,
        "object": "chat.completion",
        "model": response.model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": match response.stop {
                StopReason::ToolUse => "tool_calls",
                StopReason::MaxTokens => "length",
                StopReason::EndTurn | StopReason::StopSequence => "stop",
            },
        }],
        "usage": super::super::usage::chat_json(&response.usage),
    }))
}

fn render_chat_calls(response: &CanonicalResponse) -> Result<Vec<Value>, TransformError> {
    let mut calls = Vec::new();
    for part in &response.content {
        let ResponsePart::ToolCall {
            id,
            name,
            namespace,
            kind,
            input,
        } = part
        else {
            continue;
        };
        calls.push(json!({
            "id": id,
            "type": "function",
            "function": {
                "name": render_target_name(
                    UpstreamProtocol::ChatCompletions,
                    namespace.as_deref(),
                    name,
                    *kind,
                )?,
                "arguments": serde_json::to_string(&if *kind == ToolKind::Custom { json!({"input": custom_input(input)?}) } else { input.clone() }).map_err(|_| TransformError("无法编码工具参数".to_string()))?,
            },
        }));
    }
    Ok(calls)
}

pub(super) fn render_anthropic(response: &CanonicalResponse) -> Result<Value, TransformError> {
    let content: Vec<Value> = response
        .content
        .iter()
        .map(|part| -> Result<Value, TransformError> {
            Ok(match part {
                ResponsePart::Text(text) => json!({ "type": "text", "text": text }),
                ResponsePart::Reasoning(reasoning) => {
                    json!({ "type": "redacted_thinking", "data": reasoning.continuation })
                }
                ResponsePart::ToolCall {
                    id,
                    name,
                    namespace,
                    kind,
                    input,
                } => json!({
                    "type": "tool_use",
                    "id": id,
                    "name": render_target_name(
                        UpstreamProtocol::AnthropicMessages,
                        namespace.as_deref(),
                        name,
                        *kind,
                    )?,
                    "input": if *kind == ToolKind::Custom { json!({"input": custom_input(input)?}) } else { input.clone() },
                }),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "id": response.id,
        "type": "message",
        "role": "assistant",
        "model": response.model,
        "content": content,
        "stop_reason": match response.stop {
            StopReason::ToolUse => "tool_use",
            StopReason::MaxTokens => "max_tokens",
            StopReason::StopSequence => "stop_sequence",
            StopReason::EndTurn => "end_turn",
        },
        "stop_sequence": null,
        "usage": super::super::usage::anthropic_json(&response.usage),
    }))
}

pub(super) fn custom_input(input: &Value) -> Result<String, TransformError> {
    match input {
        Value::String(value) => Ok(value.clone()),
        Value::Object(map) if map.len() == 1 => map
            .get("input")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| TransformError("custom 工具输入必须是字符串".to_string())),
        _ => Err(TransformError("custom 工具输入必须是字符串".to_string())),
    }
}
