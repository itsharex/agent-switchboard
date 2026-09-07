//! Incremental SSE conversion for the two client-facing protocols.
//!
//! A routing gateway must not wait for an entire generation before it starts
//! responding. This reader validates one upstream SSE frame at a time and
//! releases each corresponding client frame as soon as it is available.

#[path = "stream/anthropic/mod.rs"]
mod anthropic;
#[path = "stream/chat.rs"]
mod chat;
#[path = "stream/responses/mod.rs"]
mod responses;

use super::sse::render_event;
use super::{CanonicalResponse, ReasoningTransport, StopReason, TransformError};
use asb_core::contracts::UpstreamProtocol;
use serde_json::{json, Value};
use std::io::{self, Read};

const SOURCE_READ_CHUNK: usize = 8 * 1024;

/// A bounded, one-way transcode reader. `tiny_http` pulls from it while it
/// writes a chunked client response, which means an upstream token delta is
/// not held until the upstream closes the connection.
pub(crate) struct SseTranscoder<R> {
    source: R,
    mode: StreamMode,
    target: UpstreamProtocol,
    max_source_bytes: u64,
    source_bytes: u64,
    source_buffer: Vec<u8>,
    pending: Vec<u8>,
    pending_offset: usize,
    source_finished: bool,
    terminal: bool,
}

enum StreamMode {
    Direct,
    Converting(StreamTransformer),
}

enum StreamTransformer {
    ChatToResponses(chat::ChatToResponses),
    ChatToAnthropic(chat::ChatToAnthropic),
    AnthropicToResponses(anthropic::AnthropicToResponses),
    ResponsesToAnthropic(responses::ResponsesToAnthropic),
}

impl StreamTransformer {
    fn new(
        from: UpstreamProtocol,
        to: UpstreamProtocol,
        reasoning_transport: Option<ReasoningTransport>,
    ) -> Result<Self, TransformError> {
        match (from, to) {
            (UpstreamProtocol::ChatCompletions, UpstreamProtocol::Responses) => Ok(
                Self::ChatToResponses(chat::ChatToResponses::new(reasoning_transport)),
            ),
            (UpstreamProtocol::ChatCompletions, UpstreamProtocol::AnthropicMessages) => Ok(
                Self::ChatToAnthropic(chat::ChatToAnthropic::new(reasoning_transport)),
            ),
            (UpstreamProtocol::AnthropicMessages, UpstreamProtocol::Responses) => Ok(
                Self::AnthropicToResponses(anthropic::AnthropicToResponses::default()),
            ),
            (UpstreamProtocol::Responses, UpstreamProtocol::AnthropicMessages) => Ok(
                Self::ResponsesToAnthropic(responses::ResponsesToAnthropic::default()),
            ),
            _ => Err(TransformError("该 SSE 协议组合不支持转换".to_string())),
        }
    }

    fn on_frame(&mut self, frame: Frame) -> Result<Vec<u8>, TransformError> {
        match self {
            Self::ChatToResponses(transformer) => transformer.on_frame(frame),
            Self::ChatToAnthropic(transformer) => transformer.on_frame(frame),
            Self::AnthropicToResponses(transformer) => transformer.on_frame(frame),
            Self::ResponsesToAnthropic(transformer) => transformer.on_frame(frame),
        }
    }

    fn finish(&mut self) -> Result<Vec<u8>, TransformError> {
        match self {
            Self::ChatToResponses(transformer) => transformer.finish(),
            Self::ChatToAnthropic(transformer) => transformer.finish(),
            Self::AnthropicToResponses(transformer) => transformer.finish(),
            Self::ResponsesToAnthropic(transformer) => transformer.finish(),
        }
    }
}

impl<R> SseTranscoder<R>
where
    R: Read,
{
    pub(crate) fn new(
        source: R,
        from: UpstreamProtocol,
        target: UpstreamProtocol,
        max_source_bytes: u64,
        reasoning_transport: Option<&ReasoningTransport>,
    ) -> Result<Self, TransformError> {
        let mode = if from == target {
            StreamMode::Direct
        } else {
            StreamMode::Converting(StreamTransformer::new(
                from,
                target,
                reasoning_transport.cloned(),
            )?)
        };
        Ok(Self {
            source,
            mode,
            target,
            max_source_bytes,
            source_bytes: 0,
            source_buffer: Vec::new(),
            pending: Vec::new(),
            pending_offset: 0,
            source_finished: false,
            terminal: false,
        })
    }

    fn pending_is_empty(&self) -> bool {
        self.pending_offset == self.pending.len()
    }

    fn append(&mut self, bytes: Vec<u8>) {
        if self.pending_is_empty() {
            self.pending.clear();
            self.pending_offset = 0;
        }
        self.pending.extend(bytes);
    }

    fn fail(&mut self, message: &str) {
        if !self.terminal {
            self.append(stream_error(self.target, message));
            self.terminal = true;
        }
    }

    fn consume_source(&mut self, bytes: &[u8]) -> Result<(), io::Error> {
        self.source_bytes = self.source_bytes.saturating_add(bytes.len() as u64);
        if self.source_bytes > self.max_source_bytes {
            self.fail("上游响应超过本机协议网关限制");
            return Ok(());
        }
        self.source_buffer.extend_from_slice(bytes);
        while let Some((index, delimiter_length)) = frame_boundary(&self.source_buffer) {
            let frame_bytes = self.source_buffer[..index].to_vec();
            self.source_buffer.drain(..index + delimiter_length);
            match parse_frame(&frame_bytes).and_then(|frame| match frame {
                Some(frame) => match &mut self.mode {
                    StreamMode::Direct => Ok(Vec::new()),
                    StreamMode::Converting(transformer) => transformer.on_frame(frame),
                },
                None => Ok(Vec::new()),
            }) {
                Ok(bytes) => self.append(bytes),
                Err(error) => {
                    self.fail(&format!("无法转换上游 SSE：{error}"));
                    break;
                }
            }
            if self.terminal {
                break;
            }
        }
        Ok(())
    }

    fn finish_source(&mut self) {
        if self.terminal {
            return;
        }
        if self
            .source_buffer
            .iter()
            .any(|byte| !byte.is_ascii_whitespace())
        {
            self.fail("上游 SSE 在完整帧前结束");
            return;
        }
        let result = match &mut self.mode {
            StreamMode::Direct => Ok(Vec::new()),
            StreamMode::Converting(transformer) => transformer.finish(),
        };
        match result {
            Ok(bytes) => {
                self.append(bytes);
                self.terminal = true;
            }
            Err(error) => self.fail(&format!("无法转换上游 SSE：{error}")),
        }
    }
}

impl<R> Read for SseTranscoder<R>
where
    R: Read,
{
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if matches!(self.mode, StreamMode::Direct) {
            let read = self.source.read(output)?;
            self.source_bytes = self.source_bytes.saturating_add(read as u64);
            if self.source_bytes > self.max_source_bytes {
                return Err(io::Error::other("上游响应超过本机协议网关限制"));
            }
            return Ok(read);
        }
        while self.pending_is_empty() && !self.terminal {
            if self.source_finished {
                self.finish_source();
                break;
            }
            let mut buffer = [0_u8; SOURCE_READ_CHUNK];
            match self.source.read(&mut buffer) {
                Ok(0) => {
                    self.source_finished = true;
                    self.finish_source();
                }
                Ok(read) => self.consume_source(&buffer[..read])?,
                Err(_) => self.fail("无法读取上游 SSE 流"),
            }
        }
        if self.pending_is_empty() {
            return Ok(0);
        }
        let count = output
            .len()
            .min(self.pending.len().saturating_sub(self.pending_offset));
        output[..count]
            .copy_from_slice(&self.pending[self.pending_offset..self.pending_offset + count]);
        self.pending_offset += count;
        Ok(count)
    }
}

pub(super) struct Frame {
    pub(super) event: Option<String>,
    pub(super) data: String,
}

pub(super) fn json_data(frame: &Frame, context: &str) -> Result<Value, TransformError> {
    serde_json::from_str(&frame.data)
        .map_err(|_| TransformError(format!("{context} 不是有效 JSON")))
}

pub(super) fn append_event(output: &mut Vec<u8>, event: &str, value: Value) {
    output.extend(render_event(event, &value));
}

pub(super) fn responses_complete(response: &CanonicalResponse) -> Result<Value, TransformError> {
    super::response::render_response(UpstreamProtocol::Responses, response)
}

pub(super) fn parse_responses_complete(value: &Value) -> Result<CanonicalResponse, TransformError> {
    super::response::parse_response(UpstreamProtocol::Responses, value, None)
}

pub(super) fn anthropic_stop_reason(stop: StopReason) -> &'static str {
    match stop {
        StopReason::ToolUse => "tool_use",
        StopReason::MaxTokens => "max_tokens",
        StopReason::StopSequence => "stop_sequence",
        StopReason::EndTurn => "end_turn",
    }
}

fn stream_error(protocol: UpstreamProtocol, message: &str) -> Vec<u8> {
    match protocol {
        UpstreamProtocol::Responses => render_event(
            "response.failed",
            &json!({
                "type": "response.failed",
                "response": {
                    "object": "response",
                    "status": "failed",
                    "error": { "code": "gateway_error", "message": message },
                },
            }),
        ),
        UpstreamProtocol::AnthropicMessages => render_event(
            "error",
            &json!({
                "type": "error",
                "error": { "type": "api_error", "message": message },
            }),
        ),
        UpstreamProtocol::ChatCompletions => render_event(
            "error",
            &json!({ "error": { "type": "gateway_error", "message": message } }),
        ),
    }
}

fn frame_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let mut index = 0;
    while index < buffer.len() {
        if buffer[index..].starts_with(b"\r\n\r\n") {
            return Some((index, 4));
        }
        if buffer[index..].starts_with(b"\n\n") {
            return Some((index, 2));
        }
        index += 1;
    }
    None
}

fn parse_frame(bytes: &[u8]) -> Result<Option<Frame>, TransformError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| TransformError("上游 SSE 不是 UTF-8 文本".to_string()))?;
    let mut event = None;
    let mut data = Vec::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim_start().to_string());
        } else if line.starts_with(':') || line.trim().is_empty() {
            continue;
        } else {
            return Err(TransformError("上游 SSE 包含无效帧行".to_string()));
        }
    }
    Ok((!data.is_empty()).then(|| Frame {
        event,
        data: data.join("\n"),
    }))
}

#[cfg(test)]
mod tests;
