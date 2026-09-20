use super::*;

pub(super) fn apply(
    block: &mut Block, index: Option<u64>, delta: &Map<String, Value>, output: &mut Vec<u8>,
) -> Result<(), TransformError> {
    let kind = required_string(delta, "type", "content_block_delta.delta")?;
    match (block, kind.as_str()) {
        (Block::Text { text, .. }, "text_delta") => {
            let value = delta_string(delta, "text", "Anthropic text delta")?;
            text.push_str(&value);
            if !value.is_empty() {
                append_event(output, "response.output_text.delta", json!({
                    "type":"response.output_text.delta", "output_index":index.expect("text index"),
                    "content_index":0, "delta":value,
                }));
            }
        }
        (Block::Tool { arguments, kind, .. }, "input_json_delta") => {
            let value = delta_string(delta, "partial_json", "Anthropic tool delta")?;
            arguments.push_str(&value);
            if !value.is_empty() && *kind == ToolKind::Function {
                append_event(output, "response.function_call_arguments.delta", json!({
                    "type":"response.function_call_arguments.delta",
                    "output_index":index.expect("tool index"), "delta":value,
                }));
            }
        }
        (Block::Thinking { text, .. }, "thinking_delta") => {
            text.push_str(&delta_string(delta, "thinking", "Anthropic thinking delta")?);
        }
        (Block::Thinking { signature, .. }, "signature_delta") => {
            signature.get_or_insert_with(String::new)
                .push_str(&delta_string(delta, "signature", "Anthropic signature delta")?);
        }
        _ => return Err(TransformError("Anthropic SSE content block delta 与起始类型不匹配".into())),
    }
    Ok(())
}

fn delta_string(delta: &Map<String, Value>, field: &str, context: &str) -> Result<String, TransformError> {
    allowed(delta, &["type", field], context)?;
    required_string(delta, field, context)
}
