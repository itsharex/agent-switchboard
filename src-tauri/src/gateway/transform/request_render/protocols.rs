//! Protocol-specific request renderers from the canonical request.

use super::*;

pub(super) fn render_chat(request: &CanonicalRequest) -> Result<Value, TransformError> {
    let mut messages = Vec::new();
    if !request.system.is_empty() {
        messages.push(json!({
            "role": "system",
            "content": render_chat_content(&request.system)?,
        }));
    }
    for message in &request.messages {
        match message.role {
            Role::User => {
                let mut ordinary = Vec::new();
                for part in &message.parts {
                    if let Part::ToolResult {
                        id,
                        content,
                        is_error,
                    } = part
                    {
                        if *is_error {
                            return error(
                                "Chat Completions 不支持无损的 tool_result.is_error 映射",
                            );
                        }
                        if !ordinary.is_empty() {
                            messages.push(json!({
                                "role": "user",
                                "content": render_chat_content(&ordinary)?,
                            }));
                            ordinary.clear();
                        }
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": id,
                            "content": render_chat_content(content)?,
                        }));
                    } else {
                        if matches!(part, Part::Reasoning(_)) {
                            return error("reasoning 只能出现在 assistant 消息中");
                        }
                        ordinary.push(part.clone());
                    }
                }
                if !ordinary.is_empty() {
                    messages.push(json!({
                        "role": "user",
                        "content": render_chat_content(&ordinary)?,
                    }));
                }
            }
            Role::Assistant => {
                let mut text = Vec::new();
                let mut calls = Vec::new();
                let mut reasoning = String::new();
                for part in &message.parts {
                    match part {
                        Part::Text(_) | Part::Image(_) => text.push(part.clone()),
                        Part::Reasoning(value) => reasoning.push_str(&value.content),
                        Part::ToolCall {
                            id,
                            name,
                            namespace,
                            input,
                        } => calls.push(json!({
                            "id": id,
                            "type": "function",
                            "function": { "name": render_target_name(UpstreamProtocol::ChatCompletions, namespace.as_deref(), name)?, "arguments": serde_json::to_string(input).map_err(|_| TransformError("无法编码工具参数".to_string()))? },
                        })),
                        Part::ToolResult { .. } => return error("assistant 消息不能包含 tool_result"),
                    }
                }
                let mut output = Map::new();
                output.insert("role".to_string(), Value::String("assistant".to_string()));
                output.insert(
                    "content".to_string(),
                    if text.is_empty() {
                        Value::Null
                    } else {
                        render_chat_content(&text)?
                    },
                );
                if !calls.is_empty() {
                    output.insert("tool_calls".to_string(), Value::Array(calls));
                }
                if !reasoning.is_empty() {
                    output.insert("reasoning_content".to_string(), Value::String(reasoning));
                }
                messages.push(Value::Object(output));
            }
            Role::System => {
                if message
                    .parts
                    .iter()
                    .any(|part| !matches!(part, Part::Text(_) | Part::Image(_)))
                {
                    return error("system 消息只能包含文本或图片");
                }
                messages.push(json!({
                    "role": "system",
                    "content": render_chat_content(&message.parts)?,
                }));
            }
            Role::Developer => return error("developer 消息应在规范化阶段处理"),
        }
    }
    let mut root = Map::new();
    root.insert("model".to_string(), Value::String(request.model.clone()));
    root.insert("messages".to_string(), Value::Array(messages));
    if let Some(user_id) = &request.user_id {
        root.insert("user".to_string(), Value::String(user_id.clone()));
    }
    if let Some(reasoning_effort) = request.reasoning_effort {
        root.insert(
            "reasoning_effort".to_string(),
            Value::String(
                match reasoning_effort {
                    ReasoningEffort::Low => "low",
                    ReasoningEffort::High => "high",
                    ReasoningEffort::Max => "max",
                }
                .to_string(),
            ),
        );
    }
    insert_common_chat(&mut root, request)?;
    Ok(Value::Object(root))
}

pub(super) fn render_anthropic(request: &CanonicalRequest) -> Result<Value, TransformError> {
    let max_tokens = request.max_tokens.ok_or_else(|| {
        TransformError("转换到 Anthropic Messages 时必须提供最大输出 token 数".to_string())
    })?;
    if request.reasoning_effort.is_some() {
        return error("Chat reasoning_effort 无法无损转换到 Anthropic thinking");
    }
    let mut messages = Vec::new();
    for message in &request.messages {
        let role = match message.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
            Role::Developer => return error("developer 消息无法无损转换到 Anthropic Messages"),
        };
        if role != "assistant"
            && message
                .parts
                .iter()
                .any(|part| matches!(part, Part::Reasoning(_)))
        {
            return error("reasoning 只能出现在 assistant 消息中");
        }
        messages.push(json!({
            "role": role,
            "content": render_anthropic_content(&message.parts)?,
        }));
    }
    let mut root = Map::new();
    root.insert("model".to_string(), Value::String(request.model.clone()));
    root.insert("messages".to_string(), Value::Array(messages));
    root.insert("max_tokens".to_string(), Value::Number(max_tokens.into()));
    if let Some(user_id) = &request.user_id {
        root.insert("metadata".to_string(), json!({ "user_id": user_id }));
    }
    if !request.system.is_empty() {
        root.insert(
            "system".to_string(),
            render_anthropic_content(&request.system)?,
        );
    }
    if !request.tools.is_empty() {
        if request.tools.iter().any(|tool| tool.strict) {
            return error("Anthropic Messages 无法无损表达 strict 工具");
        }
        root.insert(
            "tools".to_string(),
            Value::Array(
                request
                    .tools
                    .iter()
                    .map(|tool| -> Result<Value, TransformError> {
                        let mut value = Map::new();
                        value.insert(
                            "name".to_string(),
                            Value::String(render_target_name(
                                UpstreamProtocol::AnthropicMessages,
                                tool.namespace.as_deref(),
                                &tool.name,
                            )?),
                        );
                        if let Some(description) = &tool.description {
                            value.insert(
                                "description".to_string(),
                                Value::String(description.clone()),
                            );
                        }
                        value.insert("input_schema".to_string(), tool.input_schema.clone());
                        Ok(Value::Object(value))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        );
        root.insert(
            "tool_choice".to_string(),
            render_anthropic_tool_choice(&request.tool_choice, request.parallel_tool_calls)?,
        );
    }
    insert_common_anthropic(&mut root, request);
    Ok(Value::Object(root))
}

pub(super) fn render_responses(request: &CanonicalRequest) -> Result<Value, TransformError> {
    if request.reasoning_effort.is_some() {
        return error("reasoning_effort 无法无损转换到 Responses 请求");
    }
    let mut input = Vec::new();
    for message in &request.messages {
        if message.parts.is_empty() {
            return error("消息不含可转换内容");
        }
        let role = match message.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
            Role::Developer => "developer",
        };
        let mut ordinary = Vec::new();
        for part in &message.parts {
            match part {
                Part::ToolResult {
                    id,
                    content,
                    is_error,
                } => {
                    if message.role != Role::User {
                        return error("tool_result 只能出现在 user 消息中");
                    }
                    if *is_error {
                        return error("Responses 不支持无损的 tool_result.is_error 映射");
                    }
                    push_responses_message(&mut input, role, &ordinary)?;
                    ordinary.clear();
                    input.push(json!({
                        "type": "function_call_output",
                        "call_id": id,
                        "output": render_responses_content(content)?,
                    }));
                }
                Part::ToolCall {
                    id,
                    name,
                    namespace,
                    input: arguments,
                } => {
                    if message.role != Role::Assistant {
                        return error("tool_call 只能出现在 assistant 消息中");
                    }
                    push_responses_message(&mut input, role, &ordinary)?;
                    ordinary.clear();
                    let mut call = Map::new();
                    call.insert(
                        "type".to_string(),
                        Value::String("function_call".to_string()),
                    );
                    call.insert("call_id".to_string(), Value::String(id.clone()));
                    call.insert("name".to_string(), Value::String(name.clone()));
                    if let Some(namespace) = namespace {
                        call.insert("namespace".to_string(), Value::String(namespace.clone()));
                    }
                    call.insert(
                        "arguments".to_string(),
                        Value::String(
                            serde_json::to_string(arguments)
                                .map_err(|_| TransformError("无法编码工具参数".to_string()))?,
                        ),
                    );
                    input.push(Value::Object(call));
                }
                Part::Reasoning(reasoning) => {
                    if message.role != Role::Assistant {
                        return error("reasoning 只能出现在 assistant 消息中");
                    }
                    push_responses_message(&mut input, role, &ordinary)?;
                    ordinary.clear();
                    input.push(json!({
                        "type": "reasoning",
                        "summary": [],
                        "content": [],
                        "encrypted_content": reasoning.continuation,
                    }));
                }
                Part::Text(_) | Part::Image(_) => ordinary.push(part.clone()),
            }
        }
        push_responses_message(&mut input, role, &ordinary)?;
    }
    let mut root = Map::new();
    root.insert("model".to_string(), Value::String(request.model.clone()));
    root.insert("input".to_string(), Value::Array(input));
    if let Some(user_id) = &request.user_id {
        root.insert("metadata".to_string(), json!({ "user_id": user_id }));
    }
    if !request.system.is_empty() {
        root.insert(
            "instructions".to_string(),
            Value::String(text_only(&request.system, "Responses instructions")?),
        );
    }
    if !request.tools.is_empty() {
        root.insert("tools".to_string(), render_responses_tools(&request.tools));
        root.insert(
            "tool_choice".to_string(),
            render_responses_tool_choice(&request.tool_choice),
        );
    }
    root.insert("stream".to_string(), Value::Bool(request.stream));
    if let Some(value) = request.parallel_tool_calls {
        root.insert("parallel_tool_calls".to_string(), Value::Bool(value));
    }
    if let Some(value) = request.max_tokens {
        root.insert("max_output_tokens".to_string(), Value::Number(value.into()));
    }
    if let Some(value) = &request.temperature {
        root.insert("temperature".to_string(), value.clone());
    }
    if let Some(value) = &request.top_p {
        root.insert("top_p".to_string(), value.clone());
    }
    Ok(Value::Object(root))
}

pub(super) fn push_responses_message(
    input: &mut Vec<Value>,
    role: &str,
    ordinary: &[Part],
) -> Result<(), TransformError> {
    if ordinary.is_empty() {
        return Ok(());
    }
    input.push(json!({
        "type": "message",
        "role": role,
        "content": render_responses_content(ordinary)?,
    }));
    Ok(())
}
