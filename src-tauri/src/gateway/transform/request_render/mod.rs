use super::super::tool_names::render_target_name;
use super::super::{
    error, CanonicalRequest, ImageSource, Part, ReasoningEffort, Role, Tool, ToolChoice,
    TransformError,
};
use asb_core::contracts::UpstreamProtocol;
use serde_json::{json, Map, Value};

pub(super) fn render_request(
    protocol: UpstreamProtocol,
    request: &CanonicalRequest,
) -> Result<Value, TransformError> {
    match protocol {
        UpstreamProtocol::Responses => render_responses(request),
        UpstreamProtocol::ChatCompletions => render_chat(request),
        UpstreamProtocol::AnthropicMessages => render_anthropic(request),
    }
}

mod common;
mod content;
mod protocols;

use common::*;
use content::*;
use protocols::*;
