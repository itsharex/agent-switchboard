//! Only unwrap transport fields: native Responses input and output stay lossless.
use super::*;

pub(super) fn normalize(text: &str) -> Result<NormalizedRequest, ContextError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|_| ContextError::invalid("Codex WebSocket 请求不是有效 JSON"))?;
    let session_id = session_id(&value)?;
    let mut root = object(&value, "Codex WebSocket 请求")?.clone();
    if root.remove("type").as_ref().and_then(Value::as_str) != Some("response.create") {
        return Err(ContextError::invalid(
            "Codex WebSocket 仅支持 response.create",
        ));
    }
    let model = required_nonempty_string(&root, "model", "Codex WebSocket 请求")?;
    let generate = match root.remove("generate") {
        None => true,
        Some(Value::Bool(generate)) => generate,
        _ => return Err(ContextError::invalid("generate 必须是布尔值")),
    };
    if root
        .get("stream")
        .is_some_and(|value| value != &Value::Bool(true))
    {
        return Err(ContextError::invalid(
            "Codex WebSocket 请求必须使用 stream=true",
        ));
    }
    root.insert("stream".into(), Value::Bool(true));
    let previous_response_id = optional_nonempty_string(&root, "previous_response_id")?;
    root.remove("previous_response_id");
    if !generate && previous_response_id.is_some() {
        return Err(ContextError::invalid(
            "预热请求不能引用 previous_response_id",
        ));
    }
    let input = match root.remove("input") {
        Some(Value::Array(input)) => input,
        Some(Value::String(text)) => vec![json!({"type":"message","role":"user","content":text})],
        _ => return Err(ContextError::invalid("input 必须是字符串或数组")),
    };
    Ok(NormalizedRequest {
        model,
        template: root,
        input,
        stream: true,
        generate,
        previous_response_id,
        session_id,
    })
}

pub(super) fn completed(response: &Value) -> Result<(String, Vec<Value>), ContextError> {
    let root = object(response, "response.completed.response")?;
    let id = required_nonempty_string(root, "id", "response.completed.response")?;
    if root.get("status").and_then(Value::as_str) != Some("completed")
        || root.get("error").is_some_and(|error| !error.is_null())
    {
        return Err(ContextError::invalid("response.completed 不是成功响应"));
    }
    let output = root
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| ContextError::invalid("response.completed 缺少 output 数组"))?;
    Ok((id, output.clone()))
}
