#[path = "request_native.rs"]
mod native;
#[path = "request_parse/mod.rs"]
mod parse;
#[path = "request_render/mod.rs"]
mod render;

use super::{
    anthropic_reasoning, chat_reasoning, ConvertedRequest, ReasoningTransport, TransformError,
};
use asb_core::contracts::{CodexChatReasoning, UpstreamProtocol};
use serde_json::Value;

/// Converts one request with the explicit provider-level output limit used
/// only when a Codex Responses request targets Anthropic Messages and omitted
/// its own `max_output_tokens` field. The caller owns the profile validation;
/// this function never invents an output budget.
pub(crate) fn convert_request(
    from: UpstreamProtocol,
    to: UpstreamProtocol,
    body: &[u8],
    default_max_output_tokens: Option<u64>,
    reasoning_transport: Option<&ReasoningTransport>,
    chat_reasoning: Option<&CodexChatReasoning>,
) -> Result<ConvertedRequest, TransformError> {
    if to == UpstreamProtocol::GeminiGenerateContent && from != UpstreamProtocol::AnthropicMessages
    {
        return Err(TransformError(
            "Gemini Native 只支持 Claude Messages 转换".into(),
        ));
    }
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|_| TransformError("请求体不是有效 JSON".to_string()))?;
    if from == to {
        return native::convert(from, value, body, reasoning_transport);
    }
    if from == UpstreamProtocol::Responses {
        crate::gateway::compaction::expand_input(
            &mut value,
            reasoning_transport.map(|transport| transport.continuation_key()),
        )?;
        crate::gateway::compaction::reject_unbridgeable_input(&value)?;
    }
    let chat_reasoning = chat_reasoning::extract(from, to, &mut value, chat_reasoning)?;
    let anthropic_reasoning = anthropic_reasoning::extract(from, to, &mut value)?;
    let original = value.clone();
    if to == UpstreamProtocol::GeminiGenerateContent {
        if let Some(root) = value.as_object_mut() {
            root.remove("top_k");
        }
    }
    let mut request = parse::parse_request(from, &value, reasoning_transport)?;
    if request.max_tokens.is_none() && to == UpstreamProtocol::AnthropicMessages {
        request.max_tokens = default_max_output_tokens;
    }
    let mut value = render::render_request(to, &request)?;
    if to == UpstreamProtocol::GeminiGenerateContent {
        super::claude_gemini::generation(&original, &mut value)?;
    }
    anthropic_reasoning::render(&mut value, anthropic_reasoning)?;
    chat_reasoning::render(&mut value, chat_reasoning)?;
    Ok(ConvertedRequest {
        body: serde_json::to_vec(&value)
            .map_err(|_| TransformError("无法编码转换后的请求".to_string()))?,
        stream: request.stream,
    })
}
