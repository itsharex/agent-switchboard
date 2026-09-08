//! Loss-aware translation between the three supported provider protocols.
//!
//! Cross-protocol conversion accepts only fields with an explicit canonical
//! representation. Unknown or lossy fields fail before the upstream request
//! is made instead of being silently discarded.

pub(crate) mod minimal;
mod reasoning;
mod request;
mod response;
mod sse;
mod stream;
mod tool_names;

use serde_json::Value;

pub(crate) use reasoning::{Reasoning, ReasoningTransport};
pub(crate) use request::convert_request;
pub(crate) use response::{convert_error, convert_response};
pub(crate) use stream::SseTranscoder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransformError(pub(crate) String);

impl std::fmt::Display for TransformError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for TransformError {}

#[derive(Clone)]
pub(crate) struct ConvertedRequest {
    pub(crate) body: Vec<u8>,
    pub(crate) stream: bool,
}

#[derive(Clone)]
pub(crate) struct CanonicalRequest {
    pub(crate) model: String,
    pub(crate) system: Vec<Part>,
    pub(crate) messages: Vec<Message>,
    pub(crate) tools: Vec<Tool>,
    pub(crate) tool_choice: ToolChoice,
    /// `None` preserves the target protocol default; `Some(false)` requires
    /// a serial-only tool policy where the target can express one.
    pub(crate) parallel_tool_calls: Option<bool>,
    pub(crate) stream: bool,
    pub(crate) max_tokens: Option<u64>,
    pub(crate) temperature: Option<Value>,
    pub(crate) top_p: Option<Value>,
    pub(crate) stop: Option<Vec<String>>,
    /// The supported cross-protocol user attribution field. It maps between
    /// Anthropic/Responses `metadata.user_id` and Chat Completions `user`.
    pub(crate) user_id: Option<String>,
    /// Cross-protocol reasoning intensity. Anthropic adaptive thinking maps
    /// to Kimi's supported maximum effort.
    pub(crate) reasoning_effort: Option<ReasoningEffort>,
}

#[derive(Clone, Copy)]
pub(crate) enum ReasoningEffort {
    Low,
    High,
    Max,
}

#[derive(Clone)]
pub(crate) struct Message {
    pub(crate) role: Role,
    pub(crate) parts: Vec<Part>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    System,
    Developer,
    User,
    Assistant,
}

#[derive(Clone)]
pub(crate) enum Part {
    Text(String),
    Image(ImageSource),
    Reasoning(Reasoning),
    ToolCall {
        id: String,
        name: String,
        namespace: Option<String>,
        input: Value,
    },
    ToolResult {
        id: String,
        content: Vec<Part>,
        is_error: bool,
    },
}

#[derive(Clone)]
pub(crate) enum ImageSource {
    Data { media_type: String, data: String },
    Url(String),
}

#[derive(Clone)]
pub(crate) struct Tool {
    pub(crate) name: String,
    /// Responses can group functions under a namespace. The other two
    /// protocols have only a flat function identifier, so renderers encode
    /// this value reversibly when needed.
    pub(crate) namespace: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) input_schema: Value,
    /// OpenAI strict JSON-schema enforcement. Anthropic Messages has no
    /// equivalent, so an enabled value is rejected at its render boundary.
    pub(crate) strict: bool,
}

#[derive(Clone)]
pub(crate) enum ToolChoice {
    Auto,
    Required,
    None,
    Named {
        name: String,
        namespace: Option<String>,
    },
}

#[derive(Clone)]
pub(crate) struct CanonicalResponse {
    pub(crate) id: String,
    pub(crate) model: String,
    pub(crate) content: Vec<ResponsePart>,
    pub(crate) stop: StopReason,
    pub(crate) usage: Usage,
}

#[derive(Clone)]
pub(crate) enum ResponsePart {
    Text(String),
    Reasoning(Reasoning),
    ToolCall {
        id: String,
        name: String,
        namespace: Option<String>,
        input: Value,
    },
}

#[derive(Clone, Copy)]
pub(crate) enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
}

#[derive(Clone, Default)]
pub(crate) struct Usage {
    pub(crate) input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    pub(crate) total_tokens: Option<u64>,
}

pub(crate) fn error<T>(message: impl Into<String>) -> Result<T, TransformError> {
    Err(TransformError(message.into()))
}
