//! Keep tool failures visible and move Chat images outside text-only results.

use super::*;

pub(super) const TOOL_RESULT_ERROR_MARKER: &str = "[asb:tool-result-error]";
pub(super) const TOOL_RESULT_MEDIA_MARKER: &str =
    "[asb: tool result media moved to the following user message]";

pub(super) fn render_chat_tool_result(
    id: &str,
    content: &[Part],
    is_error: bool,
) -> Result<(Value, Vec<Part>), TransformError> {
    let mut text = Vec::new();
    let mut media = Vec::new();
    if is_error {
        text.push(Part::Text(format!("{TOOL_RESULT_ERROR_MARKER}\n")));
    }
    for part in content {
        match part {
            Part::Text(_) => text.push(part.clone()),
            Part::ToolReference(tool) => {
                text.push(Part::Text(super::claude_media::reference_text(tool)))
            }
            Part::Document(document) if matches!(document.source, DocumentSource::Text(_)) => {
                text.push(Part::Text(super::claude_media::document_text(document)))
            }
            Part::Image(_) | Part::Document(_) => {
                text.push(Part::Text(format!("\n{TOOL_RESULT_MEDIA_MARKER}\n")));
                media.push(part.clone());
            }
            _ => return error(
                "Chat tool results must contain only text, images, documents or tool references",
            ),
        }
    }
    if !media.is_empty() {
        media.insert(
            0,
            Part::Text(format!("[asb: media output of tool call {id}]")),
        );
    }
    Ok((
        json!({
            "role": "tool",
            "tool_call_id": id,
            "content": render_chat_content(&text)?,
        }),
        media,
    ))
}

pub(super) fn render_responses_tool_result(
    id: &str,
    kind: ToolKind,
    content: &[Part],
    is_error: bool,
) -> Result<Value, TransformError> {
    match kind {
        ToolKind::Function => {
            let mut output = render_responses_content(content)?;
            if is_error {
                output
                    .as_array_mut()
                    .expect("Responses content is an array")
                    .insert(
                        0,
                        json!({ "type": "input_text", "text": TOOL_RESULT_ERROR_MARKER }),
                    );
            }
            Ok(json!({ "type": "function_call_output", "call_id": id, "output": output }))
        }
        ToolKind::ToolSearch if !is_error => tool_search_output_from_content(id, content),
        ToolKind::ToolSearch => error("tool_search_output cannot encode a function tool error"),
        ToolKind::Custom => error("custom_tool_call_output 无法无损转换到 Responses"),
    }
}

fn tool_search_output_from_content(id: &str, content: &[Part]) -> Result<Value, TransformError> {
    let [Part::Text(serialized)] = content else {
        return error("tool_search_output 必须保留为单个 JSON 文本结果");
    };
    let item: Value = serde_json::from_str(serialized)
        .map_err(|_| TransformError("tool_search_output 不是有效 JSON".to_string()))?;
    let item = item
        .as_object()
        .ok_or_else(|| TransformError("tool_search_output 必须是对象".to_string()))?;
    if item.get("type").and_then(Value::as_str) != Some("tool_search_output")
        || item.get("call_id").and_then(Value::as_str) != Some(id)
    {
        return error("tool_search_output 与工具调用不一致");
    }
    Ok(Value::Object(item.clone()))
}
