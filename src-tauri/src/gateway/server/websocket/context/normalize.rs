//! Request normalization and completed-response reconstruction.

use super::*;

pub(super) fn normalize_request(text: &str) -> Result<NormalizedRequest, ContextError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|_| ContextError::invalid("Codex WebSocket 请求不是有效 JSON"))?;
    let root = object(&value, "Codex WebSocket 请求")?;
    allowed(
        root,
        &[
            "type",
            "client_metadata",
            "generate",
            "include",
            "input",
            "instructions",
            "max_output_tokens",
            "model",
            "parallel_tool_calls",
            "previous_response_id",
            "prompt_cache_key",
            "reasoning",
            "store",
            "stream",
            "temperature",
            "tool_choice",
            "tools",
            "top_p",
        ],
        "Codex WebSocket 请求",
    )?;
    if required_string(root, "type", "Codex WebSocket 请求")? != "response.create" {
        return Err(ContextError::invalid(
            "Codex WebSocket 仅支持 response.create",
        ));
    }
    let model = required_nonempty_string(root, "model", "Codex WebSocket 请求")?;
    // The current Codex wire contract makes `generate` optional: ordinary
    // response creation omits it, while a websocket prewarm sends `false`.
    // Missing therefore has the protocol-defined meaning of generation; it
    // is not a gateway-selected fallback.
    let generate = match root.get("generate") {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| ContextError::invalid("Codex WebSocket 请求.generate 必须是布尔值"))?,
        None => true,
    };
    if required_bool(root, "store", "Codex WebSocket 请求")? {
        return Err(ContextError::invalid(
            "跨协议 WebSocket 请求必须使用 store=false",
        ));
    }
    let stream = required_bool(root, "stream", "Codex WebSocket 请求")?;
    if !stream {
        return Err(ContextError::invalid(
            "Codex WebSocket 请求必须使用 stream=true",
        ));
    }
    validate_transport_metadata(root)?;
    let previous_response_id = optional_nonempty_string(root, "previous_response_id")?;
    if !generate && previous_response_id.is_some() {
        return Err(ContextError::invalid(
            "generate=false 预热请求不能引用 previous_response_id",
        ));
    }
    let input = normalize_input(
        root.get("input")
            .ok_or_else(|| ContextError::invalid("Codex WebSocket 请求缺少 input"))?,
    )?;
    let mut template = Map::new();
    for field in [
        "model",
        "instructions",
        "max_output_tokens",
        "parallel_tool_calls",
        "stream",
        "temperature",
        "tool_choice",
        "tools",
        "top_p",
    ] {
        if let Some(value) = root.get(field) {
            template.insert(field.to_string(), value.clone());
        }
    }
    if let Some(instructions) = template.get("instructions") {
        if !instructions.is_string() {
            return Err(ContextError::invalid("instructions 必须是字符串"));
        }
    }
    Ok(NormalizedRequest {
        model,
        template,
        input,
        stream,
        generate,
        previous_response_id,
    })
}

pub(super) fn validate_transport_metadata(root: &Map<String, Value>) -> Result<(), ContextError> {
    if let Some(metadata) = root.get("client_metadata") {
        if !metadata.is_object() {
            return Err(ContextError::invalid("client_metadata 必须是对象"));
        }
    }
    if let Some(include) = root.get("include") {
        let include = include
            .as_array()
            .ok_or_else(|| ContextError::invalid("include 必须是字符串数组"))?;
        if include
            .iter()
            .any(|value| value.as_str() != Some("reasoning.encrypted_content"))
        {
            return Err(ContextError::invalid(
                "include 仅支持 reasoning.encrypted_content",
            ));
        }
    }
    if let Some(reasoning) = root.get("reasoning") {
        let reasoning = object(reasoning, "reasoning")?;
        allowed(reasoning, &["summary"], "reasoning")?;
        if required_string(reasoning, "summary", "reasoning")? != "auto" {
            return Err(ContextError::invalid(
                "跨协议 WebSocket 仅支持 reasoning.summary=auto",
            ));
        }
    }
    if let Some(key) = root.get("prompt_cache_key") {
        if key.as_str().is_none_or(str::is_empty) {
            return Err(ContextError::invalid("prompt_cache_key 必须是非空字符串"));
        }
    }
    Ok(())
}

pub(super) fn normalize_input(value: &Value) -> Result<Vec<Value>, ContextError> {
    let values = value
        .as_array()
        .ok_or_else(|| ContextError::invalid("input 必须是数组"))?;
    values.iter().map(normalize_input_item).collect()
}

pub(super) fn normalize_input_item(value: &Value) -> Result<Value, ContextError> {
    let item = object(value, "input 项")?;
    let kind = required_string(item, "type", "input 项")?;
    match kind.as_str() {
        "message" => {
            allowed(
                item,
                &[
                    "type",
                    "id",
                    "role",
                    "content",
                    "internal_chat_message_metadata_passthrough",
                ],
                "Codex message",
            )?;
            if let Some(id) = item.get("id") {
                nonempty_string(id, "message.id")?;
            }
            if let Some(metadata) = item.get("internal_chat_message_metadata_passthrough") {
                if !metadata.is_object() {
                    return Err(ContextError::invalid(
                        "internal_chat_message_metadata_passthrough 必须是对象",
                    ));
                }
            }
            let role = required_string(item, "role", "message")?;
            let content = item
                .get("content")
                .ok_or_else(|| ContextError::invalid("message 缺少 content"))?;
            Ok(json!({ "type": "message", "role": role, "content": content }))
        }
        "function_call_output" => {
            allowed(item, &["type", "call_id", "output"], "function_call_output")?;
            required_nonempty_string(item, "call_id", "function_call_output")?;
            let output = item
                .get("output")
                .ok_or_else(|| ContextError::invalid("function_call_output 缺少 output"))?;
            Ok(
                json!({ "type": "function_call_output", "call_id": item["call_id"], "output": output }),
            )
        }
        "function_call" => {
            allowed(
                item,
                &["type", "call_id", "name", "namespace", "arguments"],
                "function_call",
            )?;
            required_nonempty_string(item, "call_id", "function_call")?;
            required_nonempty_string(item, "name", "function_call")?;
            required_string(item, "arguments", "function_call")?;
            let namespace = optional_nonempty_string(item, "namespace")?;
            let mut output = Map::new();
            output.insert(
                "type".to_string(),
                Value::String("function_call".to_string()),
            );
            output.insert("call_id".to_string(), item["call_id"].clone());
            output.insert("name".to_string(), item["name"].clone());
            if let Some(namespace) = namespace {
                output.insert("namespace".to_string(), Value::String(namespace));
            }
            output.insert("arguments".to_string(), item["arguments"].clone());
            Ok(Value::Object(output))
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
            required_nonempty_string(item, "encrypted_content", "Responses reasoning")?;
            if let Some(id) = item.get("id") {
                nonempty_string(id, "Responses reasoning.id")?;
            }
            if let Some(status) = item.get("status") {
                if status.as_str() != Some("completed") {
                    return Err(ContextError::invalid("Responses reasoning 必须已完成"));
                }
            }
            if let Some(summary) = item.get("summary") {
                if !summary.is_array() {
                    return Err(ContextError::invalid(
                        "Responses reasoning.summary 必须是数组",
                    ));
                }
            }
            require_empty_reasoning_content(item)?;
            let mut output = Map::new();
            output.insert("type".to_string(), Value::String("reasoning".to_string()));
            output.insert(
                "encrypted_content".to_string(),
                item["encrypted_content"].clone(),
            );
            if let Some(id) = item.get("id") {
                output.insert("id".to_string(), id.clone());
            }
            if let Some(status) = item.get("status") {
                output.insert("status".to_string(), status.clone());
            }
            if let Some(summary) = item.get("summary") {
                output.insert("summary".to_string(), summary.clone());
            }
            Ok(Value::Object(output))
        }
        _ => Err(ContextError::invalid(format!(
            "input.type {kind} 不支持跨协议转换"
        ))),
    }
}

pub(super) fn normalize_completed_response(
    response: &Value,
) -> Result<(String, Vec<Value>), ContextError> {
    let response = object(response, "response.completed.response")?;
    allowed(
        response,
        &[
            "id", "object", "status", "model", "output", "usage", "error",
        ],
        "response.completed.response",
    )?;
    let id = required_nonempty_string(response, "id", "response.completed.response")?;
    if required_string(response, "object", "response.completed.response")? != "response"
        || required_string(response, "status", "response.completed.response")? != "completed"
        || response.get("error") != Some(&Value::Null)
    {
        return Err(ContextError::invalid(
            "response.completed 不是成功完成的 Responses 响应",
        ));
    }
    required_nonempty_string(response, "model", "response.completed.response")?;
    let output = response
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| ContextError::invalid("response.completed 缺少 output 数组"))?
        .iter()
        .map(normalize_completed_output)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((id, output))
}

pub(super) fn normalize_completed_output(value: &Value) -> Result<Value, ContextError> {
    let item = object(value, "response.completed output")?;
    match required_string(item, "type", "response.completed output")?.as_str() {
        "message" => {
            allowed(
                item,
                &["type", "id", "status", "role", "content"],
                "Responses output message",
            )?;
            required_nonempty_string(item, "id", "Responses output message")?;
            if required_string(item, "status", "Responses output message")? != "completed"
                || required_string(item, "role", "Responses output message")? != "assistant"
            {
                return Err(ContextError::invalid(
                    "Responses output message 必须是已完成的 assistant 消息",
                ));
            }
            let content = item
                .get("content")
                .and_then(Value::as_array)
                .ok_or_else(|| ContextError::invalid("Responses output message 缺少 content 数组"))?
                .iter()
                .map(normalize_completed_content)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(json!({ "type": "message", "role": "assistant", "content": content }))
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
            required_nonempty_string(item, "id", "Responses function_call")?;
            required_nonempty_string(item, "call_id", "Responses function_call")?;
            required_nonempty_string(item, "name", "Responses function_call")?;
            required_string(item, "arguments", "Responses function_call")?;
            if required_string(item, "status", "Responses function_call")? != "completed" {
                return Err(ContextError::invalid("Responses function_call 必须已完成"));
            }
            let namespace = optional_nonempty_string(item, "namespace")?;
            let mut output = Map::new();
            output.insert(
                "type".to_string(),
                Value::String("function_call".to_string()),
            );
            output.insert("call_id".to_string(), item["call_id"].clone());
            output.insert("name".to_string(), item["name"].clone());
            if let Some(namespace) = namespace {
                output.insert("namespace".to_string(), Value::String(namespace));
            }
            output.insert("arguments".to_string(), item["arguments"].clone());
            Ok(Value::Object(output))
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
            required_nonempty_string(item, "id", "Responses reasoning")?;
            required_nonempty_string(item, "encrypted_content", "Responses reasoning")?;
            if required_string(item, "status", "Responses reasoning")? != "completed" {
                return Err(ContextError::invalid("Responses reasoning 必须已完成"));
            }
            if !item.get("summary").is_some_and(Value::is_array) {
                return Err(ContextError::invalid(
                    "Responses reasoning.summary 必须是数组",
                ));
            }
            require_empty_reasoning_content(item)?;
            Ok(json!({
                "type": "reasoning",
                "id": item["id"],
                "status": "completed",
                "summary": item["summary"],
                "encrypted_content": item["encrypted_content"],
            }))
        }
        kind => Err(ContextError::invalid(format!(
            "Responses output.type {kind} 不支持跨协议上下文重放"
        ))),
    }
}

pub(super) fn normalize_completed_content(value: &Value) -> Result<Value, ContextError> {
    let part = object(value, "Responses output content")?;
    allowed(
        part,
        &["type", "text", "annotations"],
        "Responses output content",
    )?;
    if required_string(part, "type", "Responses output content")? != "output_text" {
        return Err(ContextError::invalid(
            "Responses output content 仅支持 output_text",
        ));
    }
    let text = required_string(part, "text", "Responses output content")?;
    if let Some(annotations) = part.get("annotations") {
        if !annotations.is_array() {
            return Err(ContextError::invalid("Responses annotations 必须是数组"));
        }
    }
    Ok(json!({ "type": "output_text", "text": text }))
}
