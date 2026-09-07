//! Protocol-specific request parsers producing the canonical request.

use super::*;

pub(super) fn parse_responses(
    value: &Value,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<CanonicalRequest, TransformError> {
    let map = object(value, "Responses 请求")?;
    allowed(
        map,
        &[
            "model",
            "input",
            "instructions",
            "tools",
            "tool_choice",
            "parallel_tool_calls",
            "stream",
            "max_output_tokens",
            "temperature",
            "top_p",
            "client_metadata",
            "include",
            "prompt_cache_key",
            "reasoning",
            "store",
            "metadata",
        ],
        "Responses 请求",
    )?;
    validate_responses_transport_metadata(map)?;
    let mut system = match map.get("instructions") {
        None => vec![],
        Some(Value::String(text)) => vec![Part::Text(text.clone())],
        Some(_) => return error("instructions 必须是字符串"),
    };
    let mut messages = Vec::new();
    match map.get("input") {
        Some(Value::String(text)) => messages.push(Message {
            role: Role::User,
            parts: vec![Part::Text(text.clone())],
        }),
        Some(input) => {
            for item in array(input, "input")? {
                let item_map = object(item, "input 项")?;
                let kind = string(item_map.get("type"), "input.type")?;
                match kind.as_str() {
                    "message" => {
                        allowed(
                            item_map,
                            &[
                                "type",
                                "id",
                                "status",
                                "role",
                                "content",
                                "internal_chat_message_metadata_passthrough",
                            ],
                            "Responses message",
                        )?;
                        if let Some(id) = item_map.get("id") {
                            string(Some(id), "Responses message.id")?;
                        }
                        if let Some(status) = item_map.get("status") {
                            if status.as_str() != Some("completed") {
                                return error("Responses message.status 必须是 completed");
                            }
                        }
                        if let Some(metadata) =
                            item_map.get("internal_chat_message_metadata_passthrough")
                        {
                            if !metadata.is_object() {
                                return error(
                                    "internal_chat_message_metadata_passthrough 必须是对象",
                                );
                            }
                        }
                        let role = parse_role(&string(item_map.get("role"), "input.role")?)?;
                        let parts =
                            parse_response_content(item_map.get("content").ok_or_else(|| {
                                TransformError("Responses message 缺少 content".to_string())
                            })?)?;
                        if matches!(role, Role::System | Role::Developer) {
                            system.extend(parts);
                        } else if role == Role::Assistant {
                            append_assistant_parts(&mut messages, parts);
                        } else {
                            messages.push(Message { role, parts });
                        }
                    }
                    "function_call_output" => {
                        allowed(
                            item_map,
                            &["type", "call_id", "output"],
                            "function_call_output",
                        )?;
                        let id = string(item_map.get("call_id"), "function_call_output.call_id")?;
                        let content =
                            parse_response_content(item_map.get("output").ok_or_else(|| {
                                TransformError("function_call_output 缺少 output".to_string())
                            })?)?;
                        messages.push(Message {
                            role: Role::User,
                            parts: vec![Part::ToolResult {
                                id,
                                content,
                                is_error: false,
                            }],
                        });
                    }
                    "function_call" => {
                        allowed(
                            item_map,
                            &["type", "call_id", "name", "namespace", "arguments"],
                            "function_call",
                        )?;
                        let arguments =
                            string(item_map.get("arguments"), "function_call.arguments")?;
                        let input = serde_json::from_str(&arguments).map_err(|_| {
                            TransformError("function_call.arguments 必须是 JSON 对象".to_string())
                        })?;
                        append_assistant_parts(
                            &mut messages,
                            vec![Part::ToolCall {
                                id: string(item_map.get("call_id"), "function_call.call_id")?,
                                name: string(item_map.get("name"), "function_call.name")?,
                                namespace: optional_string(item_map, "namespace", "function_call")?,
                                input,
                            }],
                        );
                    }
                    "reasoning" => {
                        allowed(
                            item_map,
                            &[
                                "type",
                                "id",
                                "summary",
                                "content",
                                "encrypted_content",
                                "status",
                            ],
                            "Responses reasoning",
                        )?;
                        if let Some(status) = item_map.get("status") {
                            if status.as_str() != Some("completed") {
                                return error("Responses reasoning.status 必须是 completed");
                            }
                        }
                        if let Some(summary) = item_map.get("summary") {
                            if !summary.is_array() {
                                return error("Responses reasoning.summary 必须是数组");
                            }
                        }
                        if let Some(content) = item_map.get("content") {
                            if content.as_array().is_none_or(|parts| !parts.is_empty()) {
                                return error("Responses reasoning.content 必须是空数组");
                            }
                        }
                        let continuation = string(
                            item_map.get("encrypted_content"),
                            "reasoning.encrypted_content",
                        )?;
                        let transport = reasoning_transport.ok_or_else(|| {
                            TransformError("当前转换缺少本机推理续接通道".to_string())
                        })?;
                        append_assistant_parts(
                            &mut messages,
                            vec![Part::Reasoning(transport.from_continuation(continuation)?)],
                        );
                    }
                    _ => return error(format!("Responses input.type {kind} 不支持跨协议转换")),
                }
            }
        }
        None => return error("Responses 请求缺少 input"),
    }
    if messages.is_empty() && system.is_empty() {
        return error("Responses 请求不包含可转换的输入内容");
    }
    Ok(CanonicalRequest {
        model: string(map.get("model"), "model")?,
        system,
        messages,
        tools: parse_responses_tools(map.get("tools"))?,
        tool_choice: parse_responses_tool_choice(map.get("tool_choice"))?,
        parallel_tool_calls: optional_bool_option(map, "parallel_tool_calls")?,
        stream: optional_bool(map, "stream")?,
        max_tokens: optional_u64(map, "max_output_tokens")?,
        temperature: optional_number(map, "temperature")?,
        top_p: optional_number(map, "top_p")?,
        stop: None,
        user_id: parse_user_metadata(map.get("metadata"), "Responses metadata")?,
        reasoning_effort: None,
    })
}

pub(super) fn parse_chat(
    value: &Value,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<CanonicalRequest, TransformError> {
    let map = object(value, "Chat Completions 请求")?;
    allowed(
        map,
        &[
            "model",
            "messages",
            "tools",
            "tool_choice",
            "parallel_tool_calls",
            "stream",
            "max_tokens",
            "max_completion_tokens",
            "temperature",
            "top_p",
            "stop",
            "user",
            "reasoning_effort",
        ],
        "Chat Completions 请求",
    )?;
    let mut system = Vec::new();
    let mut messages = Vec::new();
    for value in array(
        map.get("messages")
            .ok_or_else(|| TransformError("Chat Completions 请求缺少 messages".to_string()))?,
        "messages",
    )? {
        let item = object(value, "message")?;
        allowed(
            item,
            &[
                "role",
                "content",
                "tool_calls",
                "tool_call_id",
                "reasoning_content",
            ],
            "Chat message",
        )?;
        let role = parse_role(&string(item.get("role"), "message.role")?)?;
        let mut parts = match item.get("content") {
            Some(Value::Null) | None => vec![],
            Some(content) => parse_chat_content(content)?,
        };
        if let Some(reasoning_content) = item.get("reasoning_content") {
            if role != Role::Assistant {
                return error("reasoning_content 只能出现在 assistant message 中");
            }
            match reasoning_content {
                Value::Null => {}
                Value::String(content) if content.is_empty() => {}
                Value::String(content) => {
                    let transport = reasoning_transport.ok_or_else(|| {
                        TransformError("当前转换缺少本机推理续接通道".to_string())
                    })?;
                    parts.insert(
                        0,
                        Part::Reasoning(transport.from_chat_content(content.clone())?),
                    );
                }
                _ => return error("reasoning_content 必须是字符串或 null"),
            }
        }
        if let Some(calls) = item.get("tool_calls") {
            if role != Role::Assistant {
                return error("tool_calls 只能出现在 assistant message 中");
            }
            parts.extend(parse_chat_tool_calls(calls)?);
        }
        if role == Role::User && item.get("tool_call_id").is_some() {
            return error("Chat tool 结果必须使用 role=tool");
        }
        if item.get("tool_call_id").is_some() {
            let id = string(item.get("tool_call_id"), "tool_call_id")?;
            parts = vec![Part::ToolResult {
                id,
                content: parts,
                is_error: false,
            }];
            messages.push(Message {
                role: Role::User,
                parts,
            });
        } else if matches!(role, Role::System | Role::Developer) {
            system.extend(parts);
        } else {
            messages.push(Message { role, parts });
        }
    }
    Ok(CanonicalRequest {
        model: string(map.get("model"), "model")?,
        system,
        messages,
        tools: parse_chat_tools(map.get("tools"))?,
        tool_choice: parse_chat_tool_choice(map.get("tool_choice"))?,
        parallel_tool_calls: optional_bool_option(map, "parallel_tool_calls")?,
        stream: optional_bool(map, "stream")?,
        max_tokens: optional_u64(map, "max_completion_tokens")?
            .or(optional_u64(map, "max_tokens")?),
        temperature: optional_number(map, "temperature")?,
        top_p: optional_number(map, "top_p")?,
        stop: parse_chat_stop_sequences(map.get("stop"))?,
        user_id: match map.get("user") {
            None => None,
            Some(value) => Some(string(Some(value), "user")?),
        },
        reasoning_effort: parse_chat_reasoning_effort(map.get("reasoning_effort"))?,
    })
}

pub(super) fn parse_anthropic(
    value: &Value,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<CanonicalRequest, TransformError> {
    let map = object(value, "Anthropic Messages 请求")?;
    allowed(
        map,
        &[
            "model",
            "messages",
            "system",
            "tools",
            "tool_choice",
            "stream",
            "max_tokens",
            "temperature",
            "top_p",
            "stop_sequences",
            "metadata",
            "thinking",
            "output_config",
        ],
        "Anthropic Messages 请求",
    )?;
    let system = match map.get("system") {
        None => vec![],
        Some(Value::String(text)) => vec![Part::Text(text.clone())],
        Some(value) => parse_anthropic_content(value, Role::System, reasoning_transport)?,
    };
    let mut messages = Vec::new();
    for value in array(
        map.get("messages")
            .ok_or_else(|| TransformError("Anthropic Messages 请求缺少 messages".to_string()))?,
        "messages",
    )? {
        let item = object(value, "Anthropic message")?;
        allowed(item, &["role", "content"], "Anthropic message")?;
        let role = match string(item.get("role"), "message.role")?.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "system" => Role::System,
            other => return error(format!("Anthropic message.role {other} 不支持")),
        };
        let content = item
            .get("content")
            .ok_or_else(|| TransformError("Anthropic message 缺少 content".to_string()))?;
        let parts = parse_anthropic_content(content, role, reasoning_transport)?;
        messages.push(Message { role, parts });
    }
    let tool_choice = parse_anthropic_tool_choice(map.get("tool_choice"))?;
    let adaptive_thinking = parse_anthropic_adaptive_thinking(map.get("thinking"))?;
    let reasoning_effort = parse_anthropic_output_effort(map.get("output_config"))?
        .or(adaptive_thinking.then_some(ReasoningEffort::Max));
    Ok(CanonicalRequest {
        model: string(map.get("model"), "model")?,
        system,
        messages,
        tools: parse_anthropic_tools(map.get("tools"))?,
        tool_choice,
        parallel_tool_calls: parse_anthropic_parallel_tool_calls(map.get("tool_choice"))?,
        stream: optional_bool(map, "stream")?,
        max_tokens: optional_u64(map, "max_tokens")?,
        temperature: optional_number(map, "temperature")?,
        top_p: optional_number(map, "top_p")?,
        stop: parse_anthropic_stop_sequences(map.get("stop_sequences"))?,
        user_id: parse_user_metadata(map.get("metadata"), "Anthropic metadata")?,
        reasoning_effort,
    })
}
