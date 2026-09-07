//! JSON accessors shared by the Anthropic SSE handlers.

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

pub(super) fn required_index(
    map: &Map<String, Value>,
    context: &str,
) -> Result<u64, TransformError> {
    map.get("index")
        .and_then(Value::as_u64)
        .ok_or_else(|| TransformError(format!("{context}.index 必须是非负整数")))
}

pub(super) fn parse_usage(value: &Value) -> Result<Usage, TransformError> {
    let map = object(value, "Anthropic SSE usage")?;
    allowed(
        map,
        &["input_tokens", "output_tokens"],
        "Anthropic SSE usage",
    )?;
    Ok(Usage {
        input_tokens: map.get("input_tokens").and_then(Value::as_u64),
        output_tokens: map.get("output_tokens").and_then(Value::as_u64),
        total_tokens: map
            .get("input_tokens")
            .and_then(Value::as_u64)
            .zip(map.get("output_tokens").and_then(Value::as_u64))
            .map(|(input, output)| input + output),
    })
}

pub(super) fn parse_stop(value: &str) -> Result<StopReason, TransformError> {
    match value {
        "tool_use" => Ok(StopReason::ToolUse),
        "max_tokens" => Ok(StopReason::MaxTokens),
        "stop_sequence" => Ok(StopReason::StopSequence),
        "end_turn" => Ok(StopReason::EndTurn),
        other => Err(TransformError(format!(
            "Anthropic SSE stop_reason {other} 不支持转换"
        ))),
    }
}
