//! JSON accessors shared by the context normalizers.

use super::*;

pub(super) fn object<'a>(
    value: &'a Value,
    context: &str,
) -> Result<&'a Map<String, Value>, ContextError> {
    value
        .as_object()
        .ok_or_else(|| ContextError::invalid(format!("{context} 必须是对象")))
}

pub(super) fn allowed(
    map: &Map<String, Value>,
    fields: &[&str],
    context: &str,
) -> Result<(), ContextError> {
    for key in map.keys() {
        if !fields.contains(&key.as_str()) {
            return Err(ContextError::invalid(format!(
                "{context} 包含无法安全转换的字段 {key}"
            )));
        }
    }
    Ok(())
}

pub(super) fn required_string(
    map: &Map<String, Value>,
    key: &str,
    context: &str,
) -> Result<String, ContextError> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ContextError::invalid(format!("{context}.{key} 必须是字符串")))
}

pub(super) fn required_nonempty_string(
    map: &Map<String, Value>,
    key: &str,
    context: &str,
) -> Result<String, ContextError> {
    let value = required_string(map, key, context)?;
    if value.is_empty() {
        return Err(ContextError::invalid(format!("{context}.{key} 不能为空")));
    }
    Ok(value)
}

pub(super) fn nonempty_string(value: &Value, context: &str) -> Result<String, ContextError> {
    let value = value
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| ContextError::invalid(format!("{context} 必须是非空字符串")))?;
    Ok(value)
}

pub(super) fn optional_nonempty_string(
    map: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, ContextError> {
    map.get(key)
        .map(|value| nonempty_string(value, key))
        .transpose()
}

pub(super) fn require_empty_reasoning_content(
    item: &Map<String, Value>,
) -> Result<(), ContextError> {
    if item
        .get("content")
        .is_some_and(|content| content.as_array().is_none_or(|parts| !parts.is_empty()))
    {
        return Err(ContextError::invalid(
            "Responses reasoning.content 必须是空数组",
        ));
    }
    Ok(())
}

pub(super) fn required_bool(
    map: &Map<String, Value>,
    key: &str,
    context: &str,
) -> Result<bool, ContextError> {
    map.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| ContextError::invalid(format!("{context}.{key} 必须是布尔值")))
}
