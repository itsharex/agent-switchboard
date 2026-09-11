use super::*;

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
        "message" => completed_message(item),
        "function_call" => completed_tool_call(item),
        "custom_tool_call" => completed_custom_tool_call(item),
        "tool_search_call" => completed_tool_search_call(item),
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

fn completed_custom_tool_call(item: &Map<String, Value>) -> Result<Value, ContextError> {
    allowed(
        item,
        &["type", "id", "call_id", "name", "input", "status"],
        "Responses custom_tool_call",
    )?;
    required_nonempty_string(item, "id", "Responses custom_tool_call")?;
    required_nonempty_string(item, "call_id", "Responses custom_tool_call")?;
    required_nonempty_string(item, "name", "Responses custom_tool_call")?;
    required_string(item, "input", "Responses custom_tool_call")?;
    if required_string(item, "status", "Responses custom_tool_call")? != "completed" {
        return Err(ContextError::invalid(
            "Responses custom_tool_call 必须已完成",
        ));
    }
    Ok(json!({
        "type": "custom_tool_call",
        "call_id": item["call_id"],
        "name": item["name"],
        "input": item["input"],
    }))
}

fn completed_tool_search_call(item: &Map<String, Value>) -> Result<Value, ContextError> {
    allowed(
        item,
        &["type", "id", "call_id", "status", "execution", "arguments"],
        "Responses tool_search_call",
    )?;
    required_nonempty_string(item, "call_id", "Responses tool_search_call")?;
    if required_string(item, "status", "Responses tool_search_call")? != "completed" {
        return Err(ContextError::invalid(
            "Responses tool_search_call 必须已完成",
        ));
    }
    if required_string(item, "execution", "Responses tool_search_call")? != "client" {
        return Err(ContextError::invalid(
            "Responses tool_search_call.execution 必须是 client",
        ));
    }
    let arguments = item
        .get("arguments")
        .ok_or_else(|| ContextError::invalid("Responses tool_search_call 缺少 arguments"))?;
    if !arguments.is_object() {
        return Err(ContextError::invalid(
            "Responses tool_search_call.arguments 必须是对象",
        ));
    }
    Ok(json!({
        "type": "tool_search_call",
        "call_id": item["call_id"],
        "status": "completed",
        "execution": "client",
        "arguments": arguments,
    }))
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

fn completed_message(item: &Map<String, Value>) -> Result<Value, ContextError> {
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

fn completed_tool_call(item: &Map<String, Value>) -> Result<Value, ContextError> {
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
