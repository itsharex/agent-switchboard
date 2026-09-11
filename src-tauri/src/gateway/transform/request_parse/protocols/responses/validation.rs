//! Validation helpers for replayable Responses input items.

use super::super::*;
use super::history::ResponseToolCall;

pub(super) fn validate_replay_metadata(
    item: &Map<String, Value>,
    context: &str,
) -> Result<(), TransformError> {
    if let Some(id) = item.get("id") {
        string(Some(id), &format!("{context}.id"))?;
    }
    if let Some(status) = item.get("status") {
        if status.as_str() != Some("completed") {
            return error(format!("{context}.status 必须是 completed"));
        }
    }
    Ok(())
}

pub(super) fn validate_completed_client_item(
    item: &Map<String, Value>,
    context: &str,
) -> Result<(), TransformError> {
    validate_replay_metadata(item, context)?;
    if item.get("status").and_then(Value::as_str) != Some("completed") {
        return error(format!("{context}.status 必须是 completed"));
    }
    if item.get("execution").and_then(Value::as_str) != Some("client") {
        return error(format!("{context}.execution 必须是 client"));
    }
    Ok(())
}

pub(super) fn validate_output_identity(
    item: &Map<String, Value>,
    call: &ResponseToolCall,
    context: &str,
) -> Result<(), TransformError> {
    validate_matching_output_field(item, "name", Some(&call.name), context)?;
    validate_matching_output_field(item, "namespace", call.namespace.as_deref(), context)
}

fn validate_matching_output_field(
    item: &Map<String, Value>,
    field: &str,
    expected: Option<&str>,
    context: &str,
) -> Result<(), TransformError> {
    let Some(value) = item.get(field) else {
        return Ok(());
    };
    let value = string(Some(value), &format!("{context}.{field}"))?;
    if expected != Some(value.as_str()) {
        return error(format!("{context}.{field} 与对应的 function_call 不一致"));
    }
    Ok(())
}

pub(super) fn optional_nonempty_string(
    item: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Option<String>, TransformError> {
    match item.get(field) {
        None => Ok(None),
        Some(value) => string(Some(value), &format!("{context}.{field}")).map(Some),
    }
}
