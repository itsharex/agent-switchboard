//! JSON value accessors shared by the request parsers.

use super::*;

pub(super) fn object<'a>(
    value: &'a Value,
    name: &str,
) -> Result<&'a Map<String, Value>, TransformError> {
    value
        .as_object()
        .ok_or_else(|| TransformError(format!("{name} 必须是对象")))
}

pub(super) fn array<'a>(value: &'a Value, name: &str) -> Result<&'a Vec<Value>, TransformError> {
    value
        .as_array()
        .ok_or_else(|| TransformError(format!("{name} 必须是数组")))
}

pub(super) fn string(value: Option<&Value>, name: &str) -> Result<String, TransformError> {
    value
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| TransformError(format!("{name} 必须是非空字符串")))
}

pub(super) fn optional_string(
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

pub(super) fn allowed(
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

pub(super) fn optional_number(
    map: &Map<String, Value>,
    key: &str,
) -> Result<Option<Value>, TransformError> {
    let Some(value) = map.get(key) else {
        return Ok(None);
    };
    if value.is_number() {
        Ok(Some(value.clone()))
    } else {
        error(format!("{key} 必须是数字"))
    }
}

pub(super) fn optional_u64(
    map: &Map<String, Value>,
    key: &str,
) -> Result<Option<u64>, TransformError> {
    let Some(value) = map.get(key) else {
        return Ok(None);
    };
    value
        .as_u64()
        .map(Some)
        .ok_or_else(|| TransformError(format!("{key} 必须是非负整数")))
}

pub(super) fn optional_bool(map: &Map<String, Value>, key: &str) -> Result<bool, TransformError> {
    match map.get(key) {
        None => Ok(false),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| TransformError(format!("{key} 必须是布尔值"))),
    }
}

pub(super) fn optional_bool_option(
    map: &Map<String, Value>,
    key: &str,
) -> Result<Option<bool>, TransformError> {
    match map.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| TransformError(format!("{key} 必须是布尔值"))),
    }
}
