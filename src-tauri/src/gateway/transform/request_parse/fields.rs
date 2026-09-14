//! Scalar request-field parsing: user metadata, reasoning effort,
//! protocol validation rules, and stop sequences.

use super::*;

pub(super) fn parse_user_metadata(
    value: Option<&Value>,
    context: &str,
) -> Result<Option<String>, TransformError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let metadata = object(value, context)?;
    allowed(metadata, &["user_id"], context)?;
    match metadata.get("user_id") {
        None => Ok(None),
        Some(value) => Ok(Some(string(Some(value), &format!("{context}.user_id"))?)),
    }
}

pub(super) fn parse_chat_reasoning_effort(
    value: Option<&Value>,
) -> Result<Option<ReasoningEffort>, TransformError> {
    let Some(value) = value else {
        return Ok(None);
    };
    match value.as_str() {
        Some("low") => Ok(Some(ReasoningEffort::Low)),
        Some("medium") => Ok(Some(ReasoningEffort::Medium)),
        Some("high") => Ok(Some(ReasoningEffort::High)),
        Some("max" | "xhigh") => Ok(Some(ReasoningEffort::Max)),
        Some(other) => error(format!("reasoning_effort {other} 不支持")),
        None => error("reasoning_effort 必须是 low、medium、high、max 或 xhigh"),
    }
}

pub(super) fn parse_anthropic_thinking(
    value: Option<&Value>,
) -> Result<Option<ReasoningEffort>, TransformError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let thinking = object(value, "Anthropic thinking")?;
    match string(thinking.get("type"), "thinking.type")?.as_str() {
        "enabled" => {
            allowed(thinking, &["type", "budget_tokens"], "Anthropic thinking")?;
            Ok(Some(match optional_u64(thinking, "budget_tokens")? {
                Some(budget) if budget < 4_000 => ReasoningEffort::Low,
                Some(budget) if budget < 16_000 => ReasoningEffort::Medium,
                _ => ReasoningEffort::High,
            }))
        }
        "adaptive" => {
            allowed(thinking, &["type"], "Anthropic thinking")?;
            Ok(Some(ReasoningEffort::Max))
        }
        "disabled" => {
            allowed(thinking, &["type"], "Anthropic thinking")?;
            // CC Switch omits effort for disabled thinking; it does not send
            // an unsupported `none` value to reasoning-only OpenAI models.
            Ok(None)
        }
        other => error(format!("Anthropic thinking.type={other} 不支持")),
    }
}

pub(super) fn parse_anthropic_output_effort(
    value: Option<&Value>,
) -> Result<Option<ReasoningEffort>, TransformError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let output = object(value, "Anthropic output_config")?;
    allowed(output, &["effort"], "Anthropic output_config")?;
    match output.get("effort") {
        None => Ok(None),
        Some(Value::String(value)) => match value.as_str() {
            "low" => Ok(Some(ReasoningEffort::Low)),
            "medium" => Ok(Some(ReasoningEffort::Medium)),
            "high" => Ok(Some(ReasoningEffort::High)),
            "max" => Ok(Some(ReasoningEffort::Max)),
            _ => error(format!(
                "Anthropic output_config.effort={value} 不支持；必须是 low、medium、high 或 max"
            )),
        },
        Some(_) => error("Anthropic output_config.effort 必须是字符串"),
    }
}

/// Prompt-cache placement affects Anthropic's serving and billing behavior,
/// not the message, tool, or generation semantics. Chat and Responses have no
/// equivalent. Only Anthropic's documented ephemeral marker is therefore
/// accepted and omitted during cross-protocol rendering.
pub(super) fn validate_anthropic_cache_control(value: &Value) -> Result<(), TransformError> {
    let cache_control = object(value, "cache_control")?;
    allowed(cache_control, &["type", "ttl"], "cache_control")?;
    if string(cache_control.get("type"), "cache_control.type")? != "ephemeral" {
        return error("仅支持 cache_control.type=ephemeral");
    }
    if let Some(ttl) = cache_control.get("ttl") {
        match ttl.as_str() {
            Some("5m" | "1h") => {}
            Some(_) => return error("cache_control.ttl 仅支持 5m 或 1h"),
            None => return error("cache_control.ttl 必须是字符串"),
        }
    }
    Ok(())
}

/// The current Codex HTTP client emits this metadata alongside a Responses
/// request. The only legal cross-protocol forms are enumerated here; all other
/// values are rejected before the request is rendered upstream.
pub(super) fn validate_responses_transport_metadata(
    map: &Map<String, Value>,
) -> Result<(), TransformError> {
    if let Some(store) = map.get("store") {
        match store.as_bool() {
            Some(false) => {}
            Some(true) => return error("跨协议 Responses 请求必须使用 store=false"),
            None => return error("store 必须是布尔值"),
        }
    }
    if let Some(metadata) = map.get("client_metadata") {
        if !metadata.is_object() {
            return error("client_metadata 必须是对象");
        }
    }
    if let Some(include) = map.get("include") {
        let include = include
            .as_array()
            .ok_or_else(|| TransformError("include 必须是字符串数组".to_string()))?;
        if include
            .iter()
            .any(|value| value.as_str() != Some("reasoning.encrypted_content"))
        {
            return error("include 仅支持 reasoning.encrypted_content");
        }
    }
    if let Some(reasoning) = map.get("reasoning") {
        let reasoning = object(reasoning, "reasoning")?;
        allowed(reasoning, &["summary"], "reasoning")?;
        if string(reasoning.get("summary"), "reasoning.summary")? != "auto" {
            return error("跨协议 Responses 请求仅支持 reasoning.summary=auto");
        }
    }
    if let Some(key) = map.get("prompt_cache_key") {
        if key.as_str().is_none_or(str::is_empty) {
            return error("prompt_cache_key 必须是非空字符串");
        }
    }
    Ok(())
}

pub(super) fn parse_chat_stop_sequences(
    value: Option<&Value>,
) -> Result<Option<Vec<String>>, TransformError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(vec![nonempty_stop(value, "stop")?])),
        Some(Value::Array(values)) => Ok(Some(
            values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| TransformError("stop 必须是字符串或字符串数组".to_string()))
                        .and_then(|value| nonempty_stop(value, "stop"))
                })
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Some(_) => error("stop 必须是字符串或字符串数组"),
    }
}

pub(super) fn parse_anthropic_stop_sequences(
    value: Option<&Value>,
) -> Result<Option<Vec<String>>, TransformError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(values)) => Ok(Some(
            values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| {
                            TransformError("stop_sequences 必须是字符串数组".to_string())
                        })
                        .and_then(|value| nonempty_stop(value, "stop_sequences"))
                })
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Some(_) => error("stop_sequences 必须是字符串数组"),
    }
}

pub(super) fn nonempty_stop(value: &str, name: &str) -> Result<String, TransformError> {
    (!value.is_empty())
        .then(|| value.to_string())
        .ok_or_else(|| TransformError(format!("{name} 不能包含空字符串")))
}
