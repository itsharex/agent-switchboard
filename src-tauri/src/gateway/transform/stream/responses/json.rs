//! JSON accessors shared by the Responses SSE handlers.

use super::*;

pub(super) fn object<'a>(
    value: &'a Value,
    context: &str,
) -> Result<&'a Map<String, Value>, TransformError> {
    value
        .as_object()
        .ok_or_else(|| TransformError(format!("{context} 必须是对象")))
}

pub(super) fn allowed(
    map: &Map<String, Value>,
    fields: &[&str],
    context: &str,
) -> Result<(), TransformError> {
    for field in map.keys() {
        if !fields.contains(&field.as_str()) {
            return Err(TransformError(format!(
                "{context} 包含无法安全转换的字段 {field}"
            )));
        }
    }
    Ok(())
}

pub(super) fn required_string(
    map: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<String, TransformError> {
    map.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| TransformError(format!("{context}.{field} 必须是非空字符串")))
}

pub(super) fn optional_string(
    map: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Option<String>, TransformError> {
    match map.get(field) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(TransformError(format!("{context}.{field} 必须是字符串"))),
    }
}

pub(super) fn required_value_string(
    value: &Value,
    context: &str,
) -> Result<String, TransformError> {
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| TransformError(format!("{context} 必须是字符串")))
}

pub(super) fn required_index(map: &Map<String, Value>, field: &str) -> Result<u64, TransformError> {
    map.get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| TransformError(format!("{field} 必须是非负整数")))
}
