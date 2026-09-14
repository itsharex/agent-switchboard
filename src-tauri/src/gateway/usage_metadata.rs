//! Protocol-neutral token metadata used by both clients and stream transcoders.
use super::transform;
use asb_core::contracts::UpstreamProtocol;
use serde_json::Value;

/// Provider-reported token counters used by both request records and stream
/// metadata. `input_tokens` includes cache reads and cache creation whenever
/// the provider reports those details.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TokenUsage {
    pub(crate) input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    pub(crate) cache_read_tokens: Option<u64>,
    pub(crate) cache_creation_tokens: Option<u64>,
    pub(crate) reasoning_tokens: Option<u64>,
}

impl TokenUsage {
    pub(crate) fn from_value(protocol: UpstreamProtocol, value: Option<&Value>) -> Self {
        let Ok(usage) = transform::parse_usage(protocol, value) else {
            return Self::default();
        };
        Self {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cache_read_tokens: usage.cached_tokens,
            cache_creation_tokens: usage.cache_creation_tokens,
            reasoning_tokens: usage.reasoning_tokens,
        }
    }

    pub(crate) fn merge_from(&mut self, update: &Self) {
        self.input_tokens = update.input_tokens.or(self.input_tokens);
        self.output_tokens = update.output_tokens.or(self.output_tokens);
        self.cache_read_tokens = update.cache_read_tokens.or(self.cache_read_tokens);
        self.cache_creation_tokens = update.cache_creation_tokens.or(self.cache_creation_tokens);
        self.reasoning_tokens = update.reasoning_tokens.or(self.reasoning_tokens);
    }
}

pub(crate) fn metadata_from_value(
    protocol: UpstreamProtocol,
    value: &Value,
) -> (Option<String>, TokenUsage) {
    let payload = match protocol {
        UpstreamProtocol::Responses => value.get("response").unwrap_or(value),
        UpstreamProtocol::AnthropicMessages => value.get("message").unwrap_or(value),
        UpstreamProtocol::ChatCompletions | UpstreamProtocol::GeminiGenerateContent => value,
    };
    (
        model_from_value(protocol, payload),
        TokenUsage::from_value(
            protocol,
            payload.get(if protocol == UpstreamProtocol::GeminiGenerateContent {
                "usageMetadata"
            } else {
                "usage"
            }),
        ),
    )
}

pub(crate) fn model_from_value(protocol: UpstreamProtocol, value: &Value) -> Option<String> {
    let value = if protocol == UpstreamProtocol::Responses {
        value.get("response").unwrap_or(value)
    } else {
        value
    };
    let model = match protocol {
        UpstreamProtocol::Responses | UpstreamProtocol::ChatCompletions => value.get("model"),
        UpstreamProtocol::GeminiGenerateContent => value.get("modelVersion"),
        UpstreamProtocol::AnthropicMessages => value.get("model").or_else(|| {
            value
                .get("message")
                .and_then(|message| message.get("model"))
        }),
    }?;
    let model = model.as_str()?.trim();
    (!model.is_empty() && model.chars().count() <= 512 && !model.chars().any(char::is_control))
        .then(|| model.to_string())
}

pub(crate) fn model_from_bytes(protocol: UpstreamProtocol, bytes: &[u8]) -> Option<String> {
    serde_json::from_slice(bytes)
        .ok()
        .and_then(|value| model_from_value(protocol, &value))
}
/// Observe raw SSE metadata without rewriting auxiliary-operation response bytes.
#[derive(Default)]
pub(crate) struct SseMetadata {
    buffer: Vec<u8>,
    discarding: bool,
}
impl SseMetadata {
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Vec<Value> {
        self.buffer.extend_from_slice(bytes);
        let mut values = Vec::new();
        while let Some((end, delimiter)) = self.buffer.iter().enumerate().find_map(|(i, _)| {
            if self.buffer[i..].starts_with(b"\r\n\r\n") {
                Some((i, 4))
            } else if self.buffer[i..].starts_with(b"\n\n") {
                Some((i, 2))
            } else {
                None
            }
        }) {
            if !self.discarding {
                if let Ok(frame) = std::str::from_utf8(&self.buffer[..end]) {
                    let data = frame
                        .lines()
                        .filter_map(|line| line.strip_prefix("data:"))
                        .map(str::trim_start)
                        .collect::<Vec<_>>()
                        .join("\n");
                    if let Ok(value) = serde_json::from_str(&data) {
                        values.push(value);
                    }
                }
            }
            self.discarding = false;
            self.buffer.drain(..end + delimiter);
        }
        if self.buffer.len() > 2 * 1024 * 1024 {
            let tail = self.buffer.split_off(self.buffer.len() - 3);
            self.buffer = tail;
            self.discarding = true;
        }
        values
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_sse_usage_survives_fragmentation_without_counting_done_as_data() {
        let mut observer = SseMetadata::default();
        let bytes = b"data: {\"model\":\"wire-model\",\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2,\"prompt_tokens_details\":{\"cached_tokens\":4}}}\r\n\r\ndata: [DONE]\n\n";
        let values: Vec<_> = bytes
            .chunks(3)
            .flat_map(|chunk| observer.push(chunk))
            .collect();
        assert_eq!(values.len(), 1);
        let (model, usage) = metadata_from_value(UpstreamProtocol::ChatCompletions, &values[0]);
        assert_eq!(model.as_deref(), Some("wire-model"));
        assert_eq!(usage.input_tokens, Some(10));
        assert_eq!(usage.cache_read_tokens, Some(4));
    }
}
