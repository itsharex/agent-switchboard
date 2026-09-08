use super::*;

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
        "message" => message_input(item),
        "function_call_output" => tool_output_input(item),
        "function_call" => tool_call_input(item),
        "reasoning" => reasoning_input(item),
        _ => Err(ContextError::invalid(format!(
            "input.type {kind} 不支持跨协议转换"
        ))),
    }
}

fn message_input(item: &Map<String, Value>) -> Result<Value, ContextError> {
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

fn tool_output_input(item: &Map<String, Value>) -> Result<Value, ContextError> {
    allowed(item, &["type", "call_id", "output"], "function_call_output")?;
    required_nonempty_string(item, "call_id", "function_call_output")?;
    let output = item
        .get("output")
        .ok_or_else(|| ContextError::invalid("function_call_output 缺少 output"))?;
    Ok(json!({ "type": "function_call_output", "call_id": item["call_id"], "output": output }))
}

fn tool_call_input(item: &Map<String, Value>) -> Result<Value, ContextError> {
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

fn reasoning_input(item: &Map<String, Value>) -> Result<Value, ContextError> {
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
