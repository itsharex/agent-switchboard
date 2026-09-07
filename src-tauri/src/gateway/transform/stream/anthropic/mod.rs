use super::{append_event, json_data, responses_complete, Frame};
use crate::gateway::transform::tool_names::parse_target_name;
use crate::gateway::transform::{
    CanonicalResponse, ResponsePart, StopReason, TransformError, Usage,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct AnthropicToResponses {
    id: Option<String>,
    model: Option<String>,
    usage: Usage,
    stop: StopReasonState,
    started: bool,
    blocks: BTreeMap<u64, Block>,
    completed: bool,
}

#[derive(Clone, Copy)]
enum StopReasonState {
    Unset,
    Value(StopReason),
}

impl Default for StopReasonState {
    fn default() -> Self {
        Self::Unset
    }
}

enum Block {
    Text {
        text: String,
        stopped: bool,
    },
    Tool {
        id: String,
        name: String,
        namespace: Option<String>,
        arguments: String,
        stopped: bool,
    },
}

impl AnthropicToResponses {
    pub(super) fn on_frame(&mut self, frame: Frame) -> Result<Vec<u8>, TransformError> {
        if self.completed {
            return Err(TransformError(
                "Anthropic SSE 在 message_stop 后继续发送数据".to_string(),
            ));
        }
        let mut output = Vec::new();
        match frame.event.as_deref() {
            Some("ping") => validate_ping(&frame)?,
            Some("message_start") => self.message_start(&frame, &mut output)?,
            Some("content_block_start") => self.block_start(&frame, &mut output)?,
            Some("content_block_delta") => self.block_delta(&frame, &mut output)?,
            Some("content_block_stop") => self.block_stop(&frame)?,
            Some("message_delta") => self.message_delta(&frame)?,
            Some("message_stop") => self.complete(&frame, &mut output)?,
            Some("error") => return Err(TransformError("上游 Anthropic SSE 返回错误".to_string())),
            Some(other) => {
                return Err(TransformError(format!(
                    "Anthropic SSE 事件 {other} 不支持转换"
                )))
            }
            None => return Err(TransformError("Anthropic SSE 缺少 event 名称".to_string())),
        }
        Ok(output)
    }

    pub(super) fn finish(&mut self) -> Result<Vec<u8>, TransformError> {
        if self.completed {
            Ok(Vec::new())
        } else {
            Err(TransformError(
                "Anthropic SSE 缺少 message_stop 事件".to_string(),
            ))
        }
    }
}

mod complete;
mod events;
mod json;

use complete::*;
use json::*;
