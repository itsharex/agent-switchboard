use super::{anthropic_stop_reason, append_event, json_data, parse_responses_complete, Frame};
use crate::gateway::transform::tool_names::render_target_name;
use crate::gateway::transform::{ResponsePart, TransformError};
use asb_core::contracts::UpstreamProtocol;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct ResponsesToAnthropic {
    id: Option<String>,
    model: Option<String>,
    started: bool,
    items: BTreeMap<u64, Item>,
    next_content_index: u64,
    completed: bool,
}

enum Item {
    Text {
        content_index: u64,
        text: String,
    },
    Tool {
        content_index: u64,
        id: String,
        name: String,
        arguments: String,
    },
}

impl ResponsesToAnthropic {
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
            Some("response.function_call_arguments.delta") => {
                self.tool_delta(&frame, &mut output)?
            }
            Some("response.content_part.done") => self.content_part_done(&frame)?,
            Some("response.output_item.done") => self.item_done(&frame)?,
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
        Ok(output)
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

use complete::*;
use json::*;
