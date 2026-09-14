//! Completion orchestration and shared incremental-state helpers.

mod content;
mod existing;
mod items;
mod output;

use super::*;
use existing::finish_item;
use items::{
    finish_custom_item, finish_function_item, finish_message_item, finish_tool_search_item,
};
pub(super) use output::emit_complete_part;

impl ResponsesToAnthropic {
    pub(super) fn item_done(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "response.output_item.done")?;
        let map = object(&value, "response.output_item.done")?;
        allowed(
            map,
            &["type", "sequence_number", "output_index", "item"],
            "response.output_item.done",
        )?;
        if required_string(map, "type", "response.output_item.done")? != "response.output_item.done"
        {
            return Err(TransformError(
                "response.output_item.done.type 无效".to_string(),
            ));
        }
        let output_index = required_index(map, "output_index")?;
        let item = object(
            map.get("item")
                .ok_or_else(|| TransformError("response.output_item.done 缺少 item".to_string()))?,
            "response.output_item.done.item",
        )?;
        if self.defer_incomplete_item(output_index, item)? {
            return Ok(());
        }
        ResponsesTerminal::Completed.validate_item(item)?;
        let state = self.items.get_mut(&output_index).ok_or_else(|| {
            TransformError("Responses output item done 找不到起始 item".to_string())
        })?;
        finish_item(state, item, output, self.reasoning_transport.as_ref())
    }

    pub(super) fn complete(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let (final_response, terminal) = lifecycle::terminal_response(frame)?;
        let canonical =
            parse_responses_complete(&final_response, self.reasoning_transport.as_ref())?;
        let final_output = final_response["output"]
            .as_array()
            .expect("parsed output array");
        self.validate_incomplete_items(final_output, terminal)?;
        merge(&mut self.id, &canonical.id, "Responses SSE id")?;
        merge(&mut self.model, &canonical.model, "Responses SSE model")?;
        self.usage.merge_from(&canonical.usage);
        self.start(output)?;
        self.flush_final(&canonical.content, final_output, terminal, output)?;
        append_event(
            output,
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": { "stop_reason": anthropic_stop_reason(canonical.stop), "stop_sequence": null },
                "usage": crate::gateway::transform::usage::anthropic_message_delta_json(&self.usage),
            }),
        );
        append_event(output, "message_stop", json!({ "type": "message_stop" }));
        self.completed = true;
        Ok(())
    }

    pub(super) fn flush_final(
        &mut self,
        expected: &[ResponsePart],
        final_output: &[Value],
        terminal: ResponsesTerminal,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let existing_out_of_range = self
            .items
            .keys()
            .any(|index| *index >= expected.len() as u64);
        if existing_out_of_range {
            return Err(TransformError(
                "Responses 完整响应少于已发送的流式输出项".to_string(),
            ));
        }
        for (index, part) in expected.iter().enumerate() {
            let output_index = index as u64;
            if self.items.contains_key(&output_index) {
                if let Some(pending) = self.pending_indexed.remove(&output_index) {
                    output.extend(pending);
                }
                self.finish_existing(output_index, &final_output[index], terminal, output)?;
                self.sync_closed(output_index);
            } else {
                emit_complete_part(output, output_index, part)?;
                self.closed_indices.insert(output_index);
            }
            self.flush_ready(output);
        }
        if !self.pending_indexed.is_empty() {
            return Err(TransformError(
                "Responses 完整响应与流式输出项顺序不一致".to_string(),
            ));
        }
        Ok(())
    }

    pub(super) fn start(&mut self, output: &mut Vec<u8>) -> Result<(), TransformError> {
        if self.started {
            return Ok(());
        }
        let id = self
            .id
            .as_deref()
            .ok_or_else(|| TransformError("Responses SSE 缺少 id".to_string()))?;
        let model = self
            .model
            .as_deref()
            .ok_or_else(|| TransformError("Responses SSE 缺少 model".to_string()))?;
        append_event(
            output,
            "message_start",
            json!({
                "type": "message_start",
                "message": {
                    "id": id,
                    "type": "message",
                    "role": "assistant",
                    "model": model,
                    "content": [],
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": crate::gateway::transform::usage::anthropic_message_start_json(&self.usage),
                },
            }),
        );
        self.started = true;
        Ok(())
    }

    pub(super) fn require_started(&self) -> Result<(), TransformError> {
        if self.started {
            Ok(())
        } else {
            Err(TransformError(
                "Responses SSE 在 response.created 前发送内容".to_string(),
            ))
        }
    }
}

pub(super) fn close_item(item: &mut Item, output: &mut Vec<u8>) {
    let (content_index, closed) = match item {
        Item::Text {
            content_index,
            closed,
            ..
        }
        | Item::Tool {
            content_index,
            closed,
            ..
        }
        | Item::CustomTool {
            content_index,
            closed,
            ..
        }
        | Item::ToolSearch {
            content_index,
            closed,
            ..
        }
        | Item::Reasoning {
            content_index,
            closed,
            ..
        } => (*content_index, closed),
    };
    if !*closed {
        append_event(
            output,
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": content_index }),
        );
        *closed = true;
    }
}

fn merge_done_item_id(
    expected: &mut Option<String>,
    incoming: Option<String>,
) -> Result<(), TransformError> {
    match (expected.as_ref(), incoming) {
        (Some(expected), Some(incoming)) if expected != &incoming => Err(TransformError(
            "Responses output item done.id 与起始 item 不一致".to_string(),
        )),
        (Some(_), None) => Err(TransformError(
            "Responses output item done 缺少起始 item 的 id".to_string(),
        )),
        (None, Some(incoming)) => {
            *expected = Some(incoming);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn custom_input(input: &Value) -> Result<String, TransformError> {
    match input {
        Value::String(value) => Ok(value.clone()),
        Value::Object(map) if map.len() == 1 => map
            .get("input")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| TransformError("custom 工具输入必须是字符串".to_string())),
        _ => Err(TransformError("custom 工具输入必须是字符串".to_string())),
    }
}

pub(super) fn append_suffix(
    current: &mut String,
    final_value: &str,
    context: &str,
) -> Result<String, TransformError> {
    let Some(suffix) = final_value.strip_prefix(current.as_str()) else {
        return Err(TransformError(format!("{context} 与已发送流式内容不一致")));
    };
    let suffix = suffix.to_string();
    current.push_str(&suffix);
    Ok(suffix)
}

pub(super) fn append_text_part(
    parts: &mut BTreeMap<u64, String>,
    combined: &mut String,
    content_index: u64,
    delta: &str,
    context: &str,
) -> Result<String, TransformError> {
    parts.entry(content_index).or_default().push_str(delta);
    recompute_text_parts(parts, combined, context)
}

pub(super) fn merge_text_part_snapshot(
    parts: &mut BTreeMap<u64, String>,
    combined: &mut String,
    content_index: u64,
    snapshot: &str,
    context: &str,
) -> Result<String, TransformError> {
    let current = parts.entry(content_index).or_default();
    if current.is_empty() {
        current.push_str(snapshot);
    } else if let Some(suffix) = snapshot.strip_prefix(current.as_str()) {
        current.push_str(suffix);
    } else if !current.starts_with(snapshot) {
        return Err(TransformError(format!("{context} 与已发送流式内容不一致")));
    }
    recompute_text_parts(parts, combined, context)
}

pub(super) fn replace_text_part(
    parts: &mut BTreeMap<u64, String>,
    combined: &mut String,
    content_index: u64,
    final_text: &str,
    context: &str,
) -> Result<String, TransformError> {
    let current = parts.entry(content_index).or_default();
    append_suffix(current, final_text, context)?;
    recompute_text_parts(parts, combined, context)
}

fn recompute_text_parts(
    parts: &BTreeMap<u64, String>,
    combined: &mut String,
    context: &str,
) -> Result<String, TransformError> {
    let mut joined = String::new();
    for value in parts.values() {
        joined.push_str(value);
    }
    let suffix = joined
        .strip_prefix(combined.as_str())
        .ok_or_else(|| TransformError(format!("{context} 与已发送流式内容不一致")))?
        .to_string();
    combined.push_str(&suffix);
    Ok(suffix)
}

pub(super) fn argument_suffix(
    current: &mut String,
    final_value: &str,
) -> Result<String, TransformError> {
    if let Some(suffix) = final_value.strip_prefix(current.as_str()) {
        let suffix = suffix.to_string();
        current.push_str(&suffix);
        return Ok(suffix);
    }
    let current_value: Value = serde_json::from_str(current)
        .map_err(|_| TransformError("Responses 流式工具参数与完整参数不一致".to_string()))?;
    let final_value_json: Value = serde_json::from_str(final_value)
        .map_err(|_| TransformError("Responses 完整工具参数不是 JSON".to_string()))?;
    if current_value == final_value_json {
        Ok(String::new())
    } else {
        Err(TransformError(
            "Responses 流式工具参数与完整参数不一致".to_string(),
        ))
    }
}

pub(super) fn merge(
    destination: &mut Option<String>,
    incoming: &str,
    context: &str,
) -> Result<(), TransformError> {
    match destination {
        Some(current) if current != incoming => {
            Err(TransformError(format!("{context} 在流中变化")))
        }
        Some(_) => Ok(()),
        None => {
            *destination = Some(incoming.to_string());
            Ok(())
        }
    }
}

fn emit_text_suffix(output: &mut Vec<u8>, content_index: u64, suffix: &str) {
    if !suffix.is_empty() {
        append_event(
            output,
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": content_index,
                "delta": { "type": "text_delta", "text": suffix },
            }),
        );
    }
}

fn emit_argument_suffix(output: &mut Vec<u8>, content_index: u64, suffix: &str) {
    if !suffix.is_empty() {
        append_event(
            output,
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": content_index,
                "delta": { "type": "input_json_delta", "partial_json": suffix },
            }),
        );
    }
}
