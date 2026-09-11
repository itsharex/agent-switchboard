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

pub(super) fn optional_nullable_string(
    map: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Option<String>, TransformError> {
    match map.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(TransformError(format!(
            "{context}.{field} 必须是字符串或 null"
        ))),
    }
}

pub(super) fn string_value(
    map: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<String, TransformError> {
    map.get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| TransformError(format!("{context}.{field} 必须是字符串")))
}

pub(super) fn event_item_id(
    map: &Map<String, Value>,
    context: &str,
) -> Result<Option<String>, TransformError> {
    let value = optional_string(map, "item_id", context)?;
    if value.as_deref().is_some_and(str::is_empty) {
        return Err(TransformError(format!(
            "{context}.item_id 必须是非空字符串"
        )));
    }
    Ok(value)
}

pub(super) fn merge_event_item_id(
    expected: &mut Option<String>,
    incoming: Option<String>,
    context: &str,
) -> Result<(), TransformError> {
    let Some(incoming) = incoming else {
        return Ok(());
    };
    match expected {
        Some(current) if current != &incoming => Err(TransformError(format!(
            "{context}.item_id 与 output item 不一致"
        ))),
        Some(_) => Ok(()),
        slot @ None => {
            *slot = Some(incoming);
            Ok(())
        }
    }
}

pub(super) fn validate_optional_array(
    map: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<(), TransformError> {
    if let Some(value) = map.get(field) {
        if !value.is_array() {
            return Err(TransformError(format!("{context}.{field} 必须是数组")));
        }
    }
    Ok(())
}

pub(super) fn validate_obfuscation(
    map: &Map<String, Value>,
    context: &str,
) -> Result<(), TransformError> {
    if let Some(value) = map.get("obfuscation") {
        if !value.is_string() {
            return Err(TransformError(format!(
                "{context}.obfuscation 必须是字符串"
            )));
        }
    }
    Ok(())
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
