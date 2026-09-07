//! Protocol-specific response parsers and their JSON accessors.

use super::*;

pub(super) fn object<'a>(
    value: &'a Value,
    name: &str,
) -> Result<&'a Map<String, Value>, TransformError> {
    value
        .as_object()
        .ok_or_else(|| TransformError(format!("{name} 必须是对象")))
}

pub(super) fn array<'a>(value: &'a Value, name: &str) -> Result<&'a Vec<Value>, TransformError> {
    value
        .as_array()
        .ok_or_else(|| TransformError(format!("{name} 必须是数组")))
}

pub(super) fn string(value: Option<&Value>, name: &str) -> Result<String, TransformError> {
    value
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| TransformError(format!("{name} 必须是字符串")))
}

pub(super) fn optional_string(
    map: &Map<String, Value>,
    key: &str,
    context: &str,
) -> Result<Option<String>, TransformError> {
    match map.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => error(format!("{context}.{key} 必须是字符串")),
    }
}

pub(super) fn allowed(
    map: &Map<String, Value>,
    fields: &[&str],
    context: &str,
) -> Result<(), TransformError> {
    for key in map.keys() {
        if !fields.contains(&key.as_str()) {
            return error(format!("{context} 包含无法安全转换的字段 {key}"));
        }
    }
    Ok(())
}

pub(super) fn parse_responses(
    value: &Value,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<CanonicalResponse, TransformError> {
    let map = object(value, "Responses 响应")?;
    allowed(
        map,
        &[
            "id",
            "object",
            "created_at",
            "status",
            "model",
            "output",
            "usage",
            "error",
            "background",
            "incomplete_details",
            "metadata",
            "parallel_tool_calls",
            "temperature",
            "tool_choice",
            "tools",
            "top_p",
            "max_output_tokens",
            "instructions",
        ],
        "Responses 响应",
    )?;
    if let Some(error_value) = map.get("error") {
        if !error_value.is_null() {
            return error("上游 Responses 返回错误");
        }
    }
    if map.get("status").and_then(Value::as_str) == Some("incomplete") {
        return error("上游 Responses 返回不完整响应，无法安全转换");
    }
    let mut content = Vec::new();
    for value in array(
        map.get("output")
            .ok_or_else(|| TransformError("Responses 响应缺少 output".to_string()))?,
        "output",
    )? {
        let item = object(value, "Responses output item")?;
        let kind = string(item.get("type"), "output.type")?;
        match kind.as_str() {
            "message" => {
                allowed(
                    item,
                    &["type", "id", "status", "role", "content"],
                    "Responses message",
                )?;
                if string(item.get("role"), "message.role")? != "assistant" {
                    return error("Responses output message 不是 assistant");
                }
                for part in array(
                    item.get("content").ok_or_else(|| {
                        TransformError("Responses message 缺少 content".to_string())
                    })?,
                    "message.content",
                )? {
                    let part = object(part, "Responses output content")?;
                    let part_type = string(part.get("type"), "content.type")?;
                    match part_type.as_str() {
                        "output_text" | "refusal" => {
                            allowed(
                                part,
                                &["type", "text", "refusal", "annotations"],
                                "Responses output text",
                            )?;
                            let field = if part_type == "refusal" {
                                "refusal"
                            } else {
                                "text"
                            };
                            content.push(ResponsePart::Text(string(part.get(field), field)?));
                        }
                        other => {
                            return error(format!(
                                "Responses output content.type {other} 不支持转换"
                            ))
                        }
                    }
                }
            }
            "function_call" => {
                allowed(
                    item,
                    &[
                        "type",
                        "id",
                        "call_id",
                        "name",
                        "namespace",
                        "arguments",
                        "status",
                    ],
                    "Responses function_call",
                )?;
                let arguments = string(item.get("arguments"), "function_call.arguments")?;
                content.push(ResponsePart::ToolCall {
                    id: string(item.get("call_id"), "function_call.call_id")?,
                    name: string(item.get("name"), "function_call.name")?,
                    namespace: optional_string(item, "namespace", "Responses function_call")?,
                    input: serde_json::from_str(&arguments).map_err(|_| {
                        TransformError("Responses function_call.arguments 不是 JSON".to_string())
                    })?,
                });
            }
            "reasoning" => {
                allowed(
                    item,
                    &[
                        "type",
                        "id",
                        "status",
                        "summary",
                        "content",
                        "encrypted_content",
                    ],
                    "Responses reasoning",
                )?;
                if let Some(status) = item.get("status") {
                    if status.as_str() != Some("completed") {
                        return error("Responses reasoning.status 必须是 completed");
                    }
                }
                if let Some(summary) = item.get("summary") {
                    if !summary.is_array() {
                        return error("Responses reasoning.summary 必须是数组");
                    }
                }
                if let Some(content) = item.get("content") {
                    if content.as_array().is_none_or(|parts| !parts.is_empty()) {
                        return error("Responses reasoning.content 必须是空数组");
                    }
                }
                let transport = reasoning_transport
                    .ok_or_else(|| TransformError("当前转换缺少本机推理续接通道".to_string()))?;
                content.push(ResponsePart::Reasoning(transport.from_continuation(
                    string(item.get("encrypted_content"), "reasoning.encrypted_content")?,
                )?));
            }
            other => return error(format!("Responses output.type {other} 不支持转换")),
        }
    }
    Ok(CanonicalResponse {
        id: string(map.get("id"), "id")?,
        model: string(map.get("model"), "model")?,
        content,
        stop: if map
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| status == "completed")
        {
            StopReason::EndTurn
        } else {
            StopReason::MaxTokens
        },
        usage: parse_responses_usage(map.get("usage"))?,
    })
}

pub(super) fn parse_chat(
    value: &Value,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<CanonicalResponse, TransformError> {
    let map = object(value, "Chat 响应")?;
    allowed(
        map,
        &[
            "id",
            "object",
            "created",
            "model",
            "choices",
            "usage",
            "system_fingerprint",
            "service_tier",
        ],
        "Chat 响应",
    )?;
    if let Some(service_tier) = map.get("service_tier") {
        if !service_tier.is_null() {
            service_tier
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    TransformError("Chat service_tier 必须是非空字符串或 null".to_string())
                })?;
        }
    }
    let choice = array(
        map.get("choices")
            .ok_or_else(|| TransformError("Chat 响应缺少 choices".to_string()))?,
        "choices",
    )?
    .first()
    .ok_or_else(|| TransformError("Chat 响应 choices 为空".to_string()))?;
    let choice = object(choice, "choice")?;
    allowed(
        choice,
        &["index", "message", "finish_reason", "logprobs"],
        "Chat choice",
    )?;
    if choice.get("logprobs").is_some() && !choice.get("logprobs").is_some_and(Value::is_null) {
        return error("Chat logprobs 无法安全转换");
    }
    let message = object(
        choice
            .get("message")
            .ok_or_else(|| TransformError("Chat choice 缺少 message".to_string()))?,
        "Chat message",
    )?;
    allowed(
        message,
        &[
            "role",
            "content",
            "tool_calls",
            "refusal",
            "reasoning_content",
        ],
        "Chat message",
    )?;
    if string(message.get("role"), "message.role")? != "assistant" {
        return error("Chat 响应 message 不是 assistant");
    }
    let mut content = Vec::new();
    if let Some(reasoning_content) = message.get("reasoning_content") {
        match reasoning_content {
            Value::Null => {}
            Value::String(value) if value.is_empty() => {}
            Value::String(value) => {
                let transport = reasoning_transport
                    .ok_or_else(|| TransformError("当前转换缺少本机推理续接通道".to_string()))?;
                content.push(ResponsePart::Reasoning(
                    transport.from_chat_content(value.clone())?,
                ));
            }
            _ => return error("Chat reasoning_content 必须是字符串或 null"),
        }
    }
    if let Some(value) = message.get("content") {
        if let Some(text) = value.as_str() {
            content.push(ResponsePart::Text(text.to_string()));
        } else if !value.is_null() {
            return error("Chat assistant content 必须是字符串或 null");
        }
    }
    if let Some(refusal) = message.get("refusal") {
        if let Some(text) = refusal.as_str() {
            content.push(ResponsePart::Text(text.to_string()));
        } else if !refusal.is_null() {
            return error("Chat refusal 必须是字符串或 null");
        }
    }
    if let Some(calls) = message.get("tool_calls") {
        for value in array(calls, "tool_calls")? {
            let call = object(value, "tool_call")?;
            allowed(call, &["id", "type", "function"], "tool_call")?;
            if string(call.get("type"), "tool_call.type")? != "function" {
                return error("Chat tool_call 仅支持 function");
            }
            let function = object(
                call.get("function")
                    .ok_or_else(|| TransformError("tool_call 缺少 function".to_string()))?,
                "tool_call.function",
            )?;
            allowed(function, &["name", "arguments"], "tool_call.function")?;
            content.push(ResponsePart::ToolCall {
                id: string(call.get("id"), "tool_call.id")?,
                name: string(function.get("name"), "tool_call.function.name")?,
                namespace: None,
                input: serde_json::from_str(&string(
                    function.get("arguments"),
                    "tool_call.function.arguments",
                )?)
                .map_err(|_| TransformError("Chat tool_call arguments 不是 JSON".to_string()))?,
            });
        }
    }
    Ok(CanonicalResponse {
        id: string(map.get("id"), "id")?,
        model: string(map.get("model"), "model")?,
        content,
        stop: parse_chat_stop(choice.get("finish_reason"))?,
        usage: parse_chat_usage(map.get("usage"))?,
    })
}

pub(super) fn parse_anthropic(
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
        let part = object(value, "Anthropic content")?;
        let kind = string(part.get("type"), "content.type")?;
        match kind.as_str() {
            "text" => {
                allowed(part, &["type", "text", "citations"], "Anthropic text")?;
                if part.get("citations").is_some()
                    && !part.get("citations").is_some_and(Value::is_null)
                {
                    return error("Anthropic citations 无法安全转换");
                }
                content.push(ResponsePart::Text(string(part.get("text"), "text")?));
            }
            "tool_use" => {
                allowed(part, &["type", "id", "name", "input"], "Anthropic tool_use")?;
                content.push(ResponsePart::ToolCall {
                    id: string(part.get("id"), "tool_use.id")?,
                    name: string(part.get("name"), "tool_use.name")?,
                    namespace: None,
                    input: part
                        .get("input")
                        .cloned()
                        .ok_or_else(|| TransformError("tool_use 缺少 input".to_string()))?,
                });
            }
            "redacted_thinking" => {
                allowed(part, &["type", "data"], "Anthropic redacted_thinking")?;
                let transport = reasoning_transport
                    .ok_or_else(|| TransformError("当前转换缺少本机推理续接通道".to_string()))?;
                content.push(ResponsePart::Reasoning(transport.from_continuation(
                    string(part.get("data"), "redacted_thinking.data")?,
                )?));
            }
            other => return error(format!("Anthropic content.type {other} 不支持转换")),
        }
    }
    Ok(CanonicalResponse {
        id: string(map.get("id"), "id")?,
        model: string(map.get("model"), "model")?,
        content,
        stop: parse_anthropic_stop(map.get("stop_reason"))?,
        usage: parse_anthropic_usage(map.get("usage"))?,
    })
}

pub(super) fn parse_responses_usage(value: Option<&Value>) -> Result<Usage, TransformError> {
    let Some(value) = value else {
        return Ok(Usage::default());
    };
    let map = object(value, "Responses usage")?;
    allowed(
        map,
        &[
            "input_tokens",
            "output_tokens",
            "total_tokens",
            "input_tokens_details",
            "output_tokens_details",
        ],
        "Responses usage",
    )?;
    Ok(Usage {
        input_tokens: map.get("input_tokens").and_then(Value::as_u64),
        output_tokens: map.get("output_tokens").and_then(Value::as_u64),
        total_tokens: map.get("total_tokens").and_then(Value::as_u64),
    })
}

pub(super) fn parse_chat_usage(value: Option<&Value>) -> Result<Usage, TransformError> {
    let Some(value) = value else {
        return Ok(Usage::default());
    };
    let map = object(value, "Chat usage")?;
    allowed(
        map,
        &[
            "prompt_tokens",
            "completion_tokens",
            "total_tokens",
            "prompt_tokens_details",
            "completion_tokens_details",
        ],
        "Chat usage",
    )?;
    Ok(Usage {
        input_tokens: map.get("prompt_tokens").and_then(Value::as_u64),
        output_tokens: map.get("completion_tokens").and_then(Value::as_u64),
        total_tokens: map.get("total_tokens").and_then(Value::as_u64),
    })
}

pub(super) fn parse_anthropic_usage(value: Option<&Value>) -> Result<Usage, TransformError> {
    let Some(value) = value else {
        return Ok(Usage::default());
    };
    let map = object(value, "Anthropic usage")?;
    allowed(
        map,
        &[
            "input_tokens",
            "output_tokens",
            "cache_creation_input_tokens",
            "cache_read_input_tokens",
        ],
        "Anthropic usage",
    )?;
    let input = map.get("input_tokens").and_then(Value::as_u64);
    let output = map.get("output_tokens").and_then(Value::as_u64);
    Ok(Usage {
        input_tokens: input,
        output_tokens: output,
        total_tokens: input.zip(output).map(|(left, right)| left + right),
    })
}

pub(super) fn parse_chat_stop(value: Option<&Value>) -> Result<StopReason, TransformError> {
    match value.and_then(Value::as_str) {
        Some("tool_calls") => Ok(StopReason::ToolUse),
        Some("length") => Ok(StopReason::MaxTokens),
        Some("stop") | None => Ok(StopReason::EndTurn),
        Some(other) => error(format!("Chat finish_reason {other} 不支持转换")),
    }
}

pub(super) fn parse_anthropic_stop(value: Option<&Value>) -> Result<StopReason, TransformError> {
    match value.and_then(Value::as_str) {
        Some("tool_use") => Ok(StopReason::ToolUse),
        Some("max_tokens") => Ok(StopReason::MaxTokens),
        Some("stop_sequence") => Ok(StopReason::StopSequence),
        Some("end_turn") | None => Ok(StopReason::EndTurn),
        Some(other) => error(format!("Anthropic stop_reason {other} 不支持转换")),
    }
}
