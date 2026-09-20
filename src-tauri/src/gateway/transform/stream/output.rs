//! Emit final Responses events from the same items kept in the terminal snapshot.

use super::*;
use crate::gateway::transform::{response, ToolKind, Usage};
use serde_json::json;

pub(super) fn text_done(
    output: &mut Vec<u8>,
    id: &str,
    index: u64,
    text: &str,
    status: &str,
) -> Value {
    let item = response::responses_text_item(id, index, text, status);
    append_event(output, "response.content_part.done", json!({
        "type":"response.content_part.done", "output_index":index,
        "content_index":0, "part":item["content"][0],
    }));
    item_done(output, index, &item);
    item
}

pub(super) fn tool_done(output: &mut Vec<u8>, index: u64, kind: ToolKind, item: &Value) {
    if kind == ToolKind::Custom {
        if item["input"].as_str().is_some_and(|input| !input.is_empty()) {
            append_event(output, "response.custom_tool_call_input.delta", json!({
                "type":"response.custom_tool_call_input.delta", "output_index":index,
                "delta":item["input"],
            }));
        }
        append_event(output, "response.custom_tool_call_input.done", json!({
            "type":"response.custom_tool_call_input.done", "output_index":index,
            "input":item["input"],
        }));
    }
    item_done(output, index, item);
}

fn item_done(output: &mut Vec<u8>, index: u64, item: &Value) {
    append_event(output, "response.output_item.done", json!({
        "type":"response.output_item.done", "output_index":index, "item":item,
    }));
}

pub(super) fn terminal(
    output: &mut Vec<u8>,
    id: &str,
    model: &str,
    stop: StopReason,
    usage: &Usage,
    items: Vec<Value>,
) {
    let response = response::responses_envelope(id, model, stop, usage, items);
    let event = if matches!(stop, StopReason::MaxTokens) {
        "response.incomplete"
    } else {
        "response.completed"
    };
    append_event(output, event, json!({"type":event,"response":response}));
}
