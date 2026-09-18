//! Responses SSE event decoding for relayed upstream streams.

use super::*;

pub(super) fn known_responses_event(kind: &str) -> bool {
    matches!(
        kind,
        "response.created"
            | "response.output_item.added"
            | "response.content_part.added"
            | "response.output_text.delta"
            | "response.function_call_arguments.delta"
            | "response.custom_tool_call_input.delta"
            | "response.custom_tool_call_input.done"
            | "response.content_part.done"
            | "response.output_item.done"
            | "response.completed"
            | "response.failed"
    )
}

#[derive(Default)]
pub(super) struct ResponseEventDecoder {
    buffer: Vec<u8>,
}

pub(super) struct ResponseEvent {
    pub(super) name: Option<String>,
    pub(super) value: Value,
}

impl ResponseEventDecoder {
    pub(super) fn push(&mut self, bytes: &[u8]) -> Result<Vec<ResponseEvent>, ()> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some((index, delimiter)) = event_boundary(&self.buffer) {
            let event = self.buffer[..index].to_vec();
            self.buffer.drain(..index + delimiter);
            if let Some(event) = parse_event(&event)? {
                events.push(event);
            }
        }
        Ok(events)
    }

    pub(super) fn finish(&self) -> Result<(), ()> {
        self.buffer
            .iter()
            .all(u8::is_ascii_whitespace)
            .then_some(())
            .ok_or(())
    }
}

pub(super) fn event_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    buffer.iter().enumerate().find_map(|(index, _)| {
        if buffer[index..].starts_with(b"\r\n\r\n") {
            Some((index, 4))
        } else if buffer[index..].starts_with(b"\n\n") {
            Some((index, 2))
        } else {
            None
        }
    })
}

pub(super) fn parse_event(bytes: &[u8]) -> Result<Option<ResponseEvent>, ()> {
    let text = std::str::from_utf8(bytes).map_err(|_| ())?;
    let mut name = None;
    let mut data = Vec::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            if name.replace(value.trim().to_string()).is_some() {
                return Err(());
            }
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim_start());
        } else if line.starts_with(':') || line.trim().is_empty() {
            continue;
        } else {
            return Err(());
        }
    }
    if data.is_empty() {
        return Ok(None);
    }
    let value = serde_json::from_str(&data.join("\n")).map_err(|_| ())?;
    Ok(Some(ResponseEvent { name, value }))
}
