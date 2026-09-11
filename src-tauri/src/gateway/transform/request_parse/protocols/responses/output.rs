//! Responses tool-result output normalization.

use super::super::*;

/// Convert a Responses tool output into canonical text/content parts.
///
/// Content arrays that ASB can represent natively keep their text/image
/// semantics. Any other array or object is retained as a JSON text value so
/// an upstream cannot observe a silently dropped tool result.
pub(super) fn parse_tool_output(
    item: &Map<String, Value>,
    context: &str,
) -> Result<Vec<Part>, TransformError> {
    let output = item
        .get("output")
        .ok_or_else(|| TransformError(format!("{context} 缺少 output")))?;
    match output {
        Value::String(text) => Ok(vec![Part::Text(text.clone())]),
        Value::Array(_) => match parse_response_content(output) {
            Ok(parts) => Ok(parts),
            Err(_) => Ok(vec![Part::Text(serialize_tool_output(output, context)?)]),
        },
        Value::Object(_) => Ok(vec![Part::Text(serialize_tool_output(output, context)?)]),
        _ => error(format!("{context}.output 必须是字符串、数组或对象")),
    }
}

fn serialize_tool_output(output: &Value, context: &str) -> Result<String, TransformError> {
    serde_json::to_string(output)
        .map_err(|_| TransformError(format!("{context}.output 无法编码为 JSON")))
}
