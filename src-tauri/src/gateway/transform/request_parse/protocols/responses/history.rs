//! Pairing state for visible Responses tool calls and their results.

use super::super::*;
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct ResponseToolCall {
    pub(super) kind: ToolKind,
    pub(super) name: String,
    pub(super) namespace: Option<String>,
}

#[derive(Default)]
pub(super) struct ResponseToolHistory {
    pending: BTreeMap<String, ResponseToolCall>,
}

impl ResponseToolHistory {
    pub(super) fn record(
        &mut self,
        call_id: String,
        call: ResponseToolCall,
    ) -> Result<(), TransformError> {
        if self.pending.insert(call_id.clone(), call).is_some() {
            return error(format!("Responses 工具调用 call_id {call_id} 重复"));
        }
        Ok(())
    }

    pub(super) fn take_output(
        &mut self,
        call_id: &str,
        expected: ToolKind,
        context: &str,
    ) -> Result<ResponseToolCall, TransformError> {
        let call = self.pending.remove(call_id).ok_or_else(|| {
            TransformError(format!(
                "{context}.call_id {call_id} 没有可配对的已完成工具调用"
            ))
        })?;
        if call.kind != expected {
            return error(format!(
                "{context}.call_id {call_id} 对应的是 {}，不能作为 {}",
                tool_kind_label(call.kind),
                tool_kind_output_label(expected)
            ));
        }
        Ok(call)
    }
}

pub(super) fn tool_kind_label(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Function => "function_call",
        ToolKind::Custom => "custom_tool_call",
        ToolKind::ToolSearch => "tool_search_call",
    }
}

pub(super) fn tool_kind_output_label(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Function => "function_call_output",
        ToolKind::Custom => "custom_tool_call_output",
        ToolKind::ToolSearch => "tool_search_output",
    }
}
