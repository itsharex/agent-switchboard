//! Request normalization and completed-response reconstruction.

use super::*;

const REQUEST_FIELDS: &[&str] = &[
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
];

pub(super) fn normalize_request(
    mode: ResponsesRequestMode,
    text: &str,
) -> Result<NormalizedRequest, ContextError> {
    let mut value: Value = serde_json::from_str(text)
        .map_err(|_| ContextError::invalid("Codex WebSocket 请求不是有效 JSON"))?;
    if mode == ResponsesRequestMode::Minimal {
        prepare_minimal(&mut value)?;
    }
    let root = object(&value, "Codex WebSocket 请求")?;
    allowed(root, REQUEST_FIELDS, "Codex WebSocket 请求")?;
    if required_string(root, "type", "Codex WebSocket 请求")? != "response.create" {
        return Err(ContextError::invalid(
            "Codex WebSocket 仅支持 response.create",
        ));
    }
    let model = required_nonempty_string(root, "model", "Codex WebSocket 请求")?;
    // Omitted generate is an ordinary response; false is local prewarm.
    let generate = match root.get("generate") {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| ContextError::invalid("Codex WebSocket 请求.generate 必须是布尔值"))?,
        None => true,
    };
    if mode == ResponsesRequestMode::Standard
        && required_bool(root, "store", "Codex WebSocket 请求")?
    {
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

fn prepare_minimal(value: &mut Value) -> Result<(), ContextError> {
    let previous = value.get("previous_response_id").cloned();
    value
        .as_object_mut()
        .map(|root| root.remove("previous_response_id"));
    normalize_minimal(value)?;
    if let Some(previous) = previous {
        value["previous_response_id"] = previous;
    }
    Ok(())
}

fn normalize_minimal(value: &mut Value) -> Result<(), ContextError> {
    let root = value
        .as_object_mut()
        .ok_or_else(|| ContextError::invalid("Codex WebSocket 请求必须是对象"))?;
    let envelope: Vec<_> = ["type", "generate"]
        .into_iter()
        .filter_map(|field| root.remove(field).map(|value| (field.to_string(), value)))
        .collect();
    crate::gateway::transform::minimal::filter_fields(root)
        .map_err(|error| ContextError::invalid(error.0))?;
    root.extend(envelope);
    Ok(())
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
