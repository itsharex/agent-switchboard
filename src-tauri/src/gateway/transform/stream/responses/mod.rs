use super::{anthropic_stop_reason, append_event, json_data, parse_responses_complete, Frame};
use crate::gateway::transform::tool_names::render_target_name;
use crate::gateway::transform::{ReasoningTransport, ResponsePart, ToolKind, TransformError};
use asb_core::contracts::UpstreamProtocol;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct ResponsesToAnthropic {
    id: Option<String>,
    model: Option<String>,
    started: bool,
    items: BTreeMap<u64, Item>,
    next_content_index: u64,
    next_output_index: u64,
    pending_indexed: BTreeMap<u64, Vec<u8>>,
    closed_indices: BTreeSet<u64>,
    completed: bool,
    reasoning_transport: Option<ReasoningTransport>,
}

enum Item {
    Text {
        content_index: u64,
        item_id: Option<String>,
        text: String,
        parts: BTreeMap<u64, String>,
        part_kinds: BTreeMap<u64, TextPartKind>,
        part_done: BTreeSet<u64>,
        closed: bool,
        item_done: bool,
    },
    Reasoning {
        content_index: u64,
        item_id: Option<String>,
        text: String,
        summary_parts: BTreeMap<u64, String>,
        content_parts: BTreeMap<u64, String>,
        summary_done: BTreeSet<u64>,
        content_done: BTreeSet<u64>,
        encrypted_content: Option<String>,
        continuation: Option<String>,
        incomplete: bool,
        emitted: bool,
        closed: bool,
        item_done: bool,
    },
    Tool {
        content_index: u64,
        item_id: Option<String>,
        id: String,
        name: String,
        source_name: String,
        namespace: Option<String>,
        arguments: String,
        closed: bool,
        item_done: bool,
    },
    CustomTool {
        content_index: u64,
        item_id: Option<String>,
        id: String,
        name: String,
        namespace: Option<String>,
        input: String,
        encoded_len: usize,
        closed: bool,
        item_done: bool,
    },
    ToolSearch {
        content_index: u64,
        item_id: Option<String>,
        id: String,
        arguments: String,
        closed: bool,
        item_done: bool,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextPartKind {
    Output,
    Refusal,
}

impl ResponsesToAnthropic {
    pub(super) fn new(reasoning_transport: Option<ReasoningTransport>) -> Self {
        Self {
            id: None,
            model: None,
            started: false,
            items: BTreeMap::new(),
            next_content_index: 0,
            next_output_index: 0,
            pending_indexed: BTreeMap::new(),
            closed_indices: BTreeSet::new(),
            completed: false,
            reasoning_transport,
        }
    }

    pub(super) fn on_frame(&mut self, frame: Frame) -> Result<Vec<u8>, TransformError> {
        if self.completed {
            return Err(TransformError(
                "Responses SSE 在 response.completed 后继续发送数据".to_string(),
            ));
        }
        let mut output = Vec::new();
        match frame.event.as_deref() {
            Some("response.created") | Some("response.in_progress") => {
                self.start_from_response(&frame, &mut output)?
            }
            Some("response.output_item.added") => self.item_added(&frame, &mut output)?,
            Some("response.content_part.added") => self.content_part_added(&frame, &mut output)?,
            Some("response.output_text.delta") => self.text_delta(&frame, &mut output)?,
            Some("response.refusal.delta") => self.refusal_delta(&frame, &mut output)?,
            Some("response.function_call_arguments.delta") => {
                self.tool_delta(&frame, &mut output)?
            }
            Some("response.custom_tool_call_input.delta") => {
                self.custom_tool_delta(&frame, &mut output)?
            }
            Some("response.custom_tool_call_input.done") => {
                self.custom_tool_done(&frame, &mut output)?
            }
            Some("response.output_text.done") => self.text_done(&frame, &mut output)?,
            Some("response.refusal.done") => self.refusal_done(&frame, &mut output)?,
            Some("response.function_call_arguments.done") => self.tool_done(&frame, &mut output)?,
            Some("response.content_part.done") => self.content_part_done(&frame, &mut output)?,
            Some("response.reasoning_summary_part.added") => self.reasoning_part_added(&frame)?,
            Some("response.reasoning_summary_part.done") => self.reasoning_part_done(&frame)?,
            Some("response.reasoning_summary_text.delta")
            | Some("response.reasoning_text.delta") => self.reasoning_text_delta(&frame)?,
            Some("response.reasoning_summary_text.done") | Some("response.reasoning_text.done") => {
                self.reasoning_text_done(&frame)?
            }
            Some("response.output_item.done") => self.item_done(&frame, &mut output)?,
            Some("response.completed") => self.complete(&frame, &mut output)?,
            Some("response.failed") | Some("error") => {
                return Err(TransformError("上游 Responses SSE 返回错误".to_string()))
            }
            Some(other) => {
                return Err(TransformError(format!(
                    "Responses SSE 事件 {other} 不支持转换"
                )))
            }
            None => return Err(TransformError("Responses SSE 缺少 event 名称".to_string())),
        }
        let output_index = indexed_output_index(&frame)?;
        if let Some(output_index) = output_index {
            let mut routed = Vec::new();
            self.queue_indexed(output_index, output)?;
            self.sync_closed(output_index);
            self.flush_ready(&mut routed);
            return Ok(routed);
        }
        Ok(output)
    }

    fn queue_indexed(&mut self, output_index: u64, bytes: Vec<u8>) -> Result<(), TransformError> {
        if output_index < self.next_output_index && !bytes.is_empty() {
            return Err(TransformError(
                "Responses SSE 输出项顺序已无法保持".to_string(),
            ));
        }
        if !bytes.is_empty() {
            self.pending_indexed
                .entry(output_index)
                .or_default()
                .extend(bytes);
        }
        Ok(())
    }

    fn sync_closed(&mut self, output_index: u64) {
        let closed = match self.items.get(&output_index) {
            Some(Item::Text { closed, .. })
            | Some(Item::Reasoning { closed, .. })
            | Some(Item::Tool { closed, .. })
            | Some(Item::CustomTool { closed, .. })
            | Some(Item::ToolSearch { closed, .. }) => *closed,
            None => false,
        };
        if closed {
            self.closed_indices.insert(output_index);
        }
    }

    fn flush_ready(&mut self, output: &mut Vec<u8>) {
        loop {
            let index = self.next_output_index;
            if let Some(bytes) = self.pending_indexed.remove(&index) {
                output.extend(bytes);
            } else if !self.closed_indices.contains(&index) {
                break;
            }
            if !self.closed_indices.contains(&index) {
                break;
            }
            self.next_output_index = index.saturating_add(1);
        }
    }

    pub(super) fn finish(&mut self) -> Result<Vec<u8>, TransformError> {
        if self.completed {
            Ok(Vec::new())
        } else {
            Err(TransformError(
                "Responses SSE 缺少 response.completed 事件".to_string(),
            ))
        }
    }

    fn start_from_response(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let event = frame.event.as_deref().expect("matched above");
        let value = json_data(frame, event)?;
        let map = object(&value, event)?;
        allowed(map, &["type", "sequence_number", "response"], event)?;
        if required_string(map, "type", event)? != event {
            return Err(TransformError(format!("{event}.type 无效")));
        }
        let response = object(
            map.get("response")
                .ok_or_else(|| TransformError(format!("{event} 缺少 response")))?,
            "response",
        )?;
        let id = required_string(response, "id", "response")?;
        let model = required_string(response, "model", "response")?;
        merge(&mut self.id, &id, "Responses SSE id")?;
        merge(&mut self.model, &model, "Responses SSE model")?;
        self.start(output)
    }
}

mod complete;
mod events;
mod json;
mod reasoning;

/// Responses custom tools carry raw text input, while Anthropic's tool-use
/// stream requires JSON object fragments. Keep the wrapper identical for
/// initial, delta, and completion paths.
fn custom_arguments(input: &str) -> Result<String, TransformError> {
    serde_json::to_string(&json!({ "input": input }))
        .map_err(|_| TransformError("无法编码 Responses custom 工具参数".to_string()))
}

fn indexed_output_index(frame: &Frame) -> Result<Option<u64>, TransformError> {
    let Some(event) = frame.event.as_deref() else {
        return Ok(None);
    };
    if matches!(
        event,
        "response.created"
            | "response.in_progress"
            | "response.completed"
            | "response.failed"
            | "response.incomplete"
            | "error"
    ) {
        return Ok(None);
    }
    let value = json_data(frame, event)?;
    let map = object(&value, event)?;
    match map.get("output_index") {
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| TransformError(format!("{event}.output_index 必须是非负整数"))),
        None => Ok(None),
    }
}

use complete::*;
use json::*;
use reasoning::*;
