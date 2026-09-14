use super::super::tool_names::render_target_name;
use super::super::{
    error, CanonicalRequest, Document, DocumentSource, ImageSource, Message, Part, Reasoning,
    ReasoningEffort, Role, Tool, ToolChoice, ToolKind, TransformError,
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
        UpstreamProtocol::GeminiGenerateContent => super::super::claude_gemini::request(request),
    }
}

mod chat;
mod claude_media;
mod common;
mod content;
mod protocols;
mod reasoning;
mod tool_results;

use chat::*;
use common::*;
use content::*;
use protocols::*;
use reasoning::*;
use tool_results::*;

#[cfg(test)]
mod claude_tests;
