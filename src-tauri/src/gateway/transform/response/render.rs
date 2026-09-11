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
                kind,
                input,
            } => {
                push_responses_text(&mut output, &response.id, &mut text);
                let mut call = Map::new();
                call.insert(
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
                        call.insert("id".to_string(), Value::String(format!("fc_{id}")));
                    }
                    ToolKind::Custom => {
                        call.insert("id".to_string(), Value::String(format!("ctc_{id}")));
                    }
                    ToolKind::ToolSearch => {}
                }
                call.insert("call_id".to_string(), Value::String(id.clone()));
                if *kind != ToolKind::ToolSearch {
                    call.insert("name".to_string(), Value::String(name.clone()));
                }
                if let Some(namespace) = namespace {
                    call.insert("namespace".to_string(), Value::String(namespace.clone()));
                }
                match kind {
                    ToolKind::Function => {
                        call.insert(
                            "arguments".to_string(),
                            Value::String(
                                serde_json::to_string(input)
                                    .map_err(|_| TransformError("无法编码工具参数".to_string()))?,
                            ),
                        );
                    }
                    ToolKind::Custom => {
                        call.insert("input".to_string(), Value::String(custom_input(input)?));
                    }
                    ToolKind::ToolSearch => {
                        if !input.is_object() {
                            return error("tool_search 参数必须是对象");
                        }
                        call.insert("execution".to_string(), Value::String("client".to_string()));
                        call.insert("arguments".to_string(), input.clone());
                    }
                }
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
        "usage": super::super::usage::responses_json(&response.usage),
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

fn custom_input(input: &Value) -> Result<String, TransformError> {
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
