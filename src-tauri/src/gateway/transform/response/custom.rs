use super::*;

pub(super) fn parse_custom_tool_call(
    item: &Map<String, Value>,
) -> Result<ResponsePart, TransformError> {
    allowed(
        item,
        &["type", "id", "call_id", "name", "input", "status"],
        "Responses custom_tool_call",
    )?;
    Ok(ResponsePart::ToolCall {
        id: string(item.get("call_id"), "custom_tool_call.call_id")?,
        name: string(item.get("name"), "custom_tool_call.name")?,
        namespace: None,
        kind: ToolKind::Custom,
        input: Value::String(string(item.get("input"), "custom_tool_call.input")?),
    })
}
