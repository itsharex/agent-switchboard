//! Shared JSON accessors and protocol-neutral response metadata parsers.
use super::*;

pub(in super::super) fn object<'a>(
    value: &'a Value,
    name: &str,
) -> Result<&'a Map<String, Value>, TransformError> {
    value
        .as_object()
        .ok_or_else(|| TransformError(format!("{name} 必须是对象")))
}

pub(in super::super) fn array<'a>(
    value: &'a Value,
    name: &str,
) -> Result<&'a Vec<Value>, TransformError> {
    value
        .as_array()
        .ok_or_else(|| TransformError(format!("{name} 必须是数组")))
}

pub(in super::super) fn string(
    value: Option<&Value>,
    name: &str,
) -> Result<String, TransformError> {
    value
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| TransformError(format!("{name} 必须是字符串")))
}

pub(in super::super) fn optional_string(
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

pub(in super::super) fn allowed(
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

pub(in super::super) fn parse_chat_stop(
    value: Option<&Value>,
) -> Result<StopReason, TransformError> {
    match value.and_then(Value::as_str) {
        Some("tool_calls") => Ok(StopReason::ToolUse),
        Some("length") => Ok(StopReason::MaxTokens),
        Some("stop") | None => Ok(StopReason::EndTurn),
        Some(other) => error(format!("Chat finish_reason {other} 不支持转换")),
    }
}

pub(in super::super) fn parse_anthropic_stop(
    value: Option<&Value>,
) -> Result<StopReason, TransformError> {
    match value.and_then(Value::as_str) {
        Some("tool_use") => Ok(StopReason::ToolUse),
        Some("max_tokens") => Ok(StopReason::MaxTokens),
        Some("stop_sequence") => Ok(StopReason::StopSequence),
        Some("end_turn") | None => Ok(StopReason::EndTurn),
        Some(other) => error(format!("Anthropic stop_reason {other} 不支持转换")),
    }
}
