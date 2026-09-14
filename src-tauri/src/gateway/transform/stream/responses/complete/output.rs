//! Emit complete parts that had no preceding streamed item.

use super::*;

pub(in crate::gateway::transform::stream::responses) fn emit_complete_part(
    output: &mut Vec<u8>,
    content_index: u64,
    part: &ResponsePart,
) -> Result<(), TransformError> {
    match part {
        ResponsePart::Text(text) => emit_complete_text(output, content_index, text),
        ResponsePart::Reasoning(reasoning) => {
            append_event(
                output,
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": content_index,
                    "content_block": {
                        "type": "redacted_thinking",
                        "data": reasoning.continuation,
                    },
                }),
            );
        }
        ResponsePart::ToolCall { .. } => emit_complete_tool(output, content_index, part)?,
    }
    append_event(
        output,
        "content_block_stop",
        json!({ "type": "content_block_stop", "index": content_index }),
    );
    Ok(())
}

fn emit_complete_text(output: &mut Vec<u8>, content_index: u64, text: &str) {
    append_event(
        output,
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": content_index,
            "content_block": { "type": "text", "text": "" },
        }),
    );
    if !text.is_empty() {
        append_event(
            output,
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": content_index,
                "delta": { "type": "text_delta", "text": text },
            }),
        );
    }
}

fn emit_complete_tool(
    output: &mut Vec<u8>,
    content_index: u64,
    part: &ResponsePart,
) -> Result<(), TransformError> {
    let ResponsePart::ToolCall {
        id,
        name,
        namespace,
        kind,
        input,
    } = part
    else {
        unreachable!("tool call part checked by caller")
    };
    let arguments = if *kind == ToolKind::Custom {
        custom_arguments(&custom_input(input)?)?
    } else {
        serde_json::to_string(input)
            .map_err(|_| TransformError("无法编码 Responses 工具参数".to_string()))?
    };
    append_event(
        output,
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": content_index,
            "content_block": {
                "type": "tool_use",
                "id": id,
                "name": render_target_name(
                    UpstreamProtocol::AnthropicMessages,
                    namespace.as_deref(),
                    name,
                    *kind,
                )?,
                "input": {},
            },
        }),
    );
    append_event(
        output,
        "content_block_delta",
        json!({
            "type": "content_block_delta",
            "index": content_index,
            "delta": { "type": "input_json_delta", "partial_json": arguments },
        }),
    );
    Ok(())
}
