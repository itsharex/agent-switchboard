use super::super::{
    error, CanonicalRequest, ImageSource, Message, Part, ReasoningEffort, ReasoningTransport, Role,
    Tool, ToolChoice, ToolKind, TransformError, CODEX_TOOL_SEARCH_NAME,
};
use asb_core::contracts::UpstreamProtocol;
use serde_json::{json, Map, Value};

pub(super) fn parse_request(
    protocol: UpstreamProtocol,
    value: &Value,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<CanonicalRequest, TransformError> {
    match protocol {
        UpstreamProtocol::Responses => parse_responses(value, reasoning_transport),
        UpstreamProtocol::ChatCompletions => parse_chat(value, reasoning_transport),
        UpstreamProtocol::AnthropicMessages => parse_anthropic(value, reasoning_transport),
    }
}

mod fields;
mod json;
mod messages;
mod protocols;
mod tools;

use fields::*;
use json::*;
use messages::*;
use protocols::{parse_anthropic, parse_chat, parse_responses};
use tools::*;

#[cfg(test)]
mod tool_search_tests;
