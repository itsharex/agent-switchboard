//! Protocol-specific response renderers from the canonical response.

use super::*;

pub(super) fn render_responses(response: &CanonicalResponse) -> Result<Value, TransformError> {
    let mut output = Vec::new();
    let mut text = String::new();
    for part in &response.content {
        match part {
            ResponsePart::Text(value) => text.push_str(value),
            ResponsePart::Reasoning(reasoning) => {
                push_responses_text(&mut output, &response.id, &mut text);
                output.push(responses_reasoning_item(
                    &response.id,
                    output.len() as u64,
                    reasoning,
                ));
            }
            ResponsePart::ToolCall {
                id,
                name,
                namespace,
                input,
            } => {
                push_responses_text(&mut output, &response.id, &mut text);
                let mut call = Map::new();
                call.insert(
                    "type".to_string(),
                    Value::String("function_call".to_string()),
                );
                call.insert("id".to_string(), Value::String(format!("fc_{id}")));
                call.insert("call_id".to_string(), Value::String(id.clone()));
                call.insert("name".to_string(), Value::String(name.clone()));
                if let Some(namespace) = namespace {
                    call.insert("namespace".to_string(), Value::String(namespace.clone()));
                }
                call.insert(
                    "arguments".to_string(),
                    Value::String(
                        serde_json::to_string(input)
                            .map_err(|_| TransformError("无法编码工具参数".to_string()))?,
                    ),
                );
                call.insert("status".to_string(), Value::String("completed".to_string()));
                output.push(Value::Object(call));
            }
        }
    }
    push_responses_text(&mut output, &response.id, &mut text);
    Ok(json!({
        "id": response.id,
        "object": "response",
        "status": "completed",
        "model": response.model,
        "output": output,
        "usage": {
            "input_tokens": response.usage.input_tokens.unwrap_or(0),
            "output_tokens": response.usage.output_tokens.unwrap_or(0),
            "total_tokens": response.usage.total_tokens.unwrap_or_else(|| response.usage.input_tokens.unwrap_or(0) + response.usage.output_tokens.unwrap_or(0)),
        },
        "error": null,
    }))
}

pub(super) fn push_responses_text(output: &mut Vec<Value>, response_id: &str, text: &mut String) {
    if text.is_empty() {
        return;
    }
    output.push(json!({
        "type": "message",
        "id": format!("msg_{response_id}"),
        "status": "completed",
        "role": "assistant",
        "content": [{ "type": "output_text", "text": text, "annotations": [] }],
    }));
    text.clear();
}

pub(crate) fn responses_reasoning_item(
    response_id: &str,
    output_index: u64,
    reasoning: &Reasoning,
) -> Value {
    json!({
        "type": "reasoning",
        "id": format!("rs_{response_id}_{output_index}"),
        "status": "completed",
        "summary": [],
        "content": [],
        "encrypted_content": reasoning.continuation,
    })
}

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
    let mut calls = Vec::new();
    for part in &response.content {
        let ResponsePart::ToolCall {
            id,
            name,
            namespace,
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
                )?,
                "arguments": serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string()),
            },
        }));
    }
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
        "usage": {
            "prompt_tokens": response.usage.input_tokens.unwrap_or(0),
            "completion_tokens": response.usage.output_tokens.unwrap_or(0),
            "total_tokens": response.usage.total_tokens.unwrap_or_else(|| response.usage.input_tokens.unwrap_or(0) + response.usage.output_tokens.unwrap_or(0)),
        },
    }))
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
                    input,
                } => json!({
                    "type": "tool_use",
                    "id": id,
                    "name": render_target_name(
                        UpstreamProtocol::AnthropicMessages,
                        namespace.as_deref(),
                        name,
                    )?,
                    "input": input,
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
        "usage": {
            "input_tokens": response.usage.input_tokens.unwrap_or(0),
            "output_tokens": response.usage.output_tokens.unwrap_or(0),
        },
    }))
}
