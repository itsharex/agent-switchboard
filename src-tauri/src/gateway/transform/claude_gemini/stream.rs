//! Incremental Gemini SSE; only replay metadata is retained until completion.
use super::super::stream::{anthropic_stop_reason, append_event, json_data, Frame};
use super::super::usage;
use super::replay::{call_id, visible, GeminiTurn};
use super::*;

pub(in crate::gateway::transform) struct GeminiToAnthropic {
    transport: Option<ReasoningTransport>,
    turn: GeminiTurn,
    id: Option<String>,
    model: Option<String>,
    started: bool,
    completed: bool,
    finish_reason: Option<String>,
    usage: Usage,
    index: u64,
    text: Option<u64>,
    visible: bool,
}

impl GeminiToAnthropic {
    pub(in crate::gateway::transform) fn new(transport: Option<ReasoningTransport>) -> Self {
        Self {
            transport,
            turn: GeminiTurn {
                parts: Vec::new(),
                call_ids: Vec::new(),
            },
            id: None,
            model: None,
            started: false,
            completed: false,
            finish_reason: None,
            usage: Usage::default(),
            index: 0,
            text: None,
            visible: false,
        }
    }

    pub(in crate::gateway::transform) fn on_frame(
        &mut self,
        frame: Frame,
    ) -> Result<Vec<u8>, TransformError> {
        if self.completed {
            return error("Gemini SSE 终止后继续发送数据");
        }
        if frame.data == "[DONE]" {
            return self.complete();
        }
        let value = json_data(&frame, "Gemini SSE")?;
        self.identity(&value)?;
        self.usage.merge_from(&usage::parse(
            asb_core::UpstreamProtocol::GeminiGenerateContent,
            value.get("usageMetadata"),
        )?);
        let mut output = Vec::new();
        if let Some(candidate) = super::response::candidate(&value)? {
            let parts = candidate.pointer("/content/parts");
            if let Some(parts) = parts {
                let parts = parts
                    .as_array()
                    .ok_or_else(|| TransformError("Gemini SSE parts 必须是数组".into()))?;
                if self.finish_reason.is_some() && !parts.is_empty() {
                    return error("Gemini SSE 结束原因之后继续生成内容");
                }
                for part in parts {
                    self.part(part, &mut output)?;
                }
            }
            if let Some(reason) = candidate
                .get("finishReason")
                .and_then(Value::as_str)
                .filter(|v| *v != "FINISH_REASON_UNSPECIFIED")
            {
                super::response::stop(candidate, !self.turn.call_ids.is_empty())?;
                if self
                    .finish_reason
                    .as_deref()
                    .is_some_and(|old| old != reason)
                {
                    return error("Gemini SSE 终止原因发生变化");
                }
                self.finish_reason = Some(reason.to_string());
            }
        }
        Ok(output)
    }

    pub(in crate::gateway::transform) fn finish(&mut self) -> Result<Vec<u8>, TransformError> {
        if self.completed {
            Ok(Vec::new())
        } else {
            self.complete()
        }
    }

    fn identity(&mut self, value: &Value) -> Result<(), TransformError> {
        for (slot, key) in [
            (&mut self.id, "responseId"),
            (&mut self.model, "modelVersion"),
        ] {
            if let Some(next) = super::response::identity(value, key)? {
                if slot.as_ref().is_some_and(|old| old != &next) {
                    return error(format!("Gemini SSE {key} 在同一响应中发生变化"));
                }
                *slot = Some(next);
            }
        }
        Ok(())
    }

    fn part(&mut self, part: &Value, output: &mut Vec<u8>) -> Result<(), TransformError> {
        let ids = call_id(part).into_iter().collect::<Vec<_>>();
        let parts = visible(std::slice::from_ref(part), &ids)?;
        self.turn.call_ids.extend(ids);
        self.turn.parts.push(part.clone());
        for part in parts {
            self.visible = true;
            self.start(output);
            match part {
                ResponsePart::Text(text) => self.push_text(&text, output),
                ResponsePart::ToolCall {
                    id, name, input, ..
                } => {
                    self.close_text(output);
                    let index = self.next_index();
                    append_event(
                        output,
                        "content_block_start",
                        json!({"type":"content_block_start","index":index,"content_block":{"type":"tool_use","id":id,"name":name,"input":{}}}),
                    );
                    append_event(
                        output,
                        "content_block_delta",
                        json!({"type":"content_block_delta","index":index,"delta":{"type":"input_json_delta","partial_json":input.to_string()}}),
                    );
                    append_event(
                        output,
                        "content_block_stop",
                        json!({"type":"content_block_stop","index":index}),
                    );
                }
                ResponsePart::Reasoning(_) => {
                    unreachable!("visible Gemini parts contain no reasoning")
                }
            }
        }
        Ok(())
    }

    fn start(&mut self, output: &mut Vec<u8>) {
        if self.started {
            return;
        }
        self.started = true;
        // A missing provider identity stays unknown in telemetry; a local
        // message ID is only a Claude framing identifier, not attribution.
        let id = self
            .id
            .clone()
            .unwrap_or_else(|| format!("msg_{}", uuid::Uuid::new_v4().simple()));
        append_event(
            output,
            "message_start",
            json!({"type":"message_start","message":{
                "id":id,"type":"message","role":"assistant","model":self.model.as_deref().unwrap_or_default(),
                "content":[],"stop_reason":null,"stop_sequence":null,"usage":usage::anthropic_message_start_json(&self.usage)
            }}),
        );
        // Reserve the first block for an opaque signature before any visible
        // output. Claude CLI selects the last content block for its final text.
        // The signature can complete later without creating a trailing block.
        let index = self.next_index();
        append_event(
            output,
            "content_block_start",
            json!({"type":"content_block_start","index":index,"content_block":{"type":"thinking","thinking":""}}),
        );
    }

    fn next_index(&mut self) -> u64 {
        let index = self.index;
        self.index += 1;
        index
    }

    fn push_text(&mut self, delta: &str, output: &mut Vec<u8>) {
        let index = if let Some(index) = self.text {
            index
        } else {
            let index = self.next_index();
            self.text = Some(index);
            append_event(
                output,
                "content_block_start",
                json!({"type":"content_block_start","index":index,"content_block":{"type":"text","text":""}}),
            );
            index
        };
        append_event(
            output,
            "content_block_delta",
            json!({"type":"content_block_delta","index":index,"delta":{"type":"text_delta","text":delta}}),
        );
    }

    fn close_text(&mut self, output: &mut Vec<u8>) {
        if let Some(index) = self.text.take() {
            append_event(
                output,
                "content_block_stop",
                json!({"type":"content_block_stop","index":index}),
            );
        }
    }

    fn complete(&mut self) -> Result<Vec<u8>, TransformError> {
        let reason = self
            .finish_reason
            .as_ref()
            .ok_or_else(|| TransformError("Gemini SSE 提前结束，缺少 finishReason".into()))?;
        let stop = super::response::stop(
            &json!({"finishReason":reason}),
            !self.turn.call_ids.is_empty(),
        )?
        .ok_or_else(|| TransformError("Gemini SSE 缺少有效终止原因".into()))?;
        if !self.visible {
            return error("Gemini SSE 没有可见回复或工具调用");
        }
        self.turn.validate()?;
        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| TransformError("Gemini SSE 缺少本机续接通道".into()))?;
        let reasoning = transport.from_gemini_turn(self.turn.clone())?;
        let mut output = Vec::new();
        append_event(
            &mut output,
            "content_block_delta",
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":reasoning.continuation}}),
        );
        append_event(
            &mut output,
            "content_block_stop",
            json!({"type":"content_block_stop","index":0}),
        );
        self.close_text(&mut output);
        append_event(
            &mut output,
            "message_delta",
            json!({"type":"message_delta","delta":{"stop_reason":anthropic_stop_reason(stop),"stop_sequence":null},"usage":usage::anthropic_json(&self.usage)}),
        );
        append_event(&mut output, "message_stop", json!({"type":"message_stop"}));
        self.completed = true;
        Ok(output)
    }
}
