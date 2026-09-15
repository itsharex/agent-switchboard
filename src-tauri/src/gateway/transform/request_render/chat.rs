//! Chat history keeps parallel tool results adjacent before extracted media.

use super::*;

pub(super) fn render_chat(request: &CanonicalRequest) -> Result<Value, TransformError> {
    let mut messages = Vec::new();
    if !request.system.is_empty() {
        messages.push(json!({
            "role": "system", "content": render_chat_content(&request.system)?,
        }));
    }
    let mut source = request.messages.iter().peekable();
    while let Some(message) = source.next() {
        if message.role == Role::User {
            let mut turn = vec![message];
            while source.peek().is_some_and(|next| next.role == Role::User) {
                turn.push(source.next().expect("peeked user message"));
            }
            render_user_turn(&mut messages, &turn)?;
        } else {
            messages.push(render_message(message)?);
        }
    }
    let mut root = Map::new();
    root.insert("model".to_string(), Value::String(request.model.clone()));
    root.insert("messages".to_string(), Value::Array(messages));
    if let Some(user_id) = &request.user_id {
        root.insert("user".to_string(), Value::String(user_id.clone()));
    }
    insert_openai_reasoning(&mut root, request, UpstreamProtocol::ChatCompletions);
    insert_common_chat(&mut root, request)?;
    Ok(Value::Object(root))
}

fn render_user_turn(messages: &mut Vec<Value>, turn: &[&Message]) -> Result<(), TransformError> {
    let mut media = Vec::new();
    let mut ordinary_messages = Vec::new();
    for message in turn {
        let mut ordinary = Vec::new();
        for part in &message.parts {
            match part {
                Part::ToolResult {
                    id,
                    content,
                    is_error,
                    ..
                } => {
                    let (result, images) = render_chat_tool_result(id, content, *is_error)?;
                    messages.push(result);
                    media.extend(images);
                }
                Part::Text(_)
                | Part::Image(_)
                | Part::Document(_)
                | Part::File { .. }
                | Part::Audio { .. }
                | Part::ToolReference(_) => ordinary.push(part.clone()),
                _ => return error("Chat user messages cannot contain reasoning or tool calls"),
            }
        }
        if !ordinary.is_empty() {
            ordinary_messages.push(json!({
                "role": "user", "content": render_chat_content(&ordinary)?,
            }));
        }
    }
    if !media.is_empty() {
        messages.push(json!({ "role": "user", "content": render_chat_content(&media)? }));
    }
    messages.extend(ordinary_messages);
    Ok(())
}

fn render_message(message: &Message) -> Result<Value, TransformError> {
    let role = match message.role {
        Role::Assistant => return render_assistant(&message.parts),
        Role::User => "user",
        Role::System => "system",
        Role::Developer => return error("developer 消息应在规范化阶段处理"),
    };
    Ok(json!({ "role": role, "content": render_chat_content(&message.parts)? }))
}

fn render_assistant(parts: &[Part]) -> Result<Value, TransformError> {
    let mut text = Vec::new();
    let mut calls = Vec::new();
    let mut reasoning = String::new();
    for part in parts {
        match part {
            Part::Text(_) | Part::Image(_) => text.push(part.clone()),
            Part::Reasoning(value) => {
                if value.content.is_empty() {
                    return error("该推理是不透明上游块，无法用于 Chat Completions 历史");
                }
                reasoning.push_str(&value.content);
            }
            Part::ToolCall {
                id,
                name,
                namespace,
                kind,
                input,
            } => {
                calls.push(render_tool_call(
                    id,
                    name,
                    namespace.as_deref(),
                    *kind,
                    input,
                )?);
            }
            Part::ToolResult { .. } => return error("assistant 消息不能包含 tool_result"),
            Part::Document(_) | Part::File { .. } | Part::Audio { .. } | Part::ToolReference(_) => {
                return error("Chat Completions 暂不支持该内容类型")
            }
        }
    }
    let mut output = json!({
        "role": "assistant",
        "content": if text.is_empty() { Value::Null } else { render_chat_content(&text)? },
    });
    if !calls.is_empty() {
        output["tool_calls"] = Value::Array(calls);
    }
    if !reasoning.is_empty() {
        output["reasoning_content"] = Value::String(reasoning);
    }
    Ok(output)
}

fn render_tool_call(
    id: &str,
    name: &str,
    namespace: Option<&str>,
    kind: ToolKind,
    input: &Value,
) -> Result<Value, TransformError> {
    let arguments = match kind {
        ToolKind::Function | ToolKind::ToolSearch => input.clone(),
        ToolKind::Custom => json!({ "input": input.as_str()
            .ok_or_else(|| TransformError("custom 工具输入必须是字符串".to_string()))? }),
    };
    Ok(json!({
        "id": id,
        "type": "function",
        "function": {
            "name": render_target_name(UpstreamProtocol::ChatCompletions, namespace, name, kind)?,
            "arguments": serde_json::to_string(&arguments)
                .map_err(|_| TransformError("无法编码工具参数".to_string()))?,
        },
    }))
}
