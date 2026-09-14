//! Protocol-specific response parsers and their JSON accessors.
use super::*;
mod accessors;
mod anthropic;
mod chat;
mod responses;

pub(super) use accessors::*;

pub(super) fn parse_responses(
    value: &Value,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<CanonicalResponse, TransformError> {
    let map = object(value, "Responses 响应")?;
    let terminal = lifecycle::ResponsesTerminal::parse(map)?;
    allowed(
        map,
        &[
            "id",
            "object",
            "created_at",
            "status",
            "model",
            "output",
            "usage",
            "error",
            "background",
            "incomplete_details",
            "metadata",
            "parallel_tool_calls",
            "temperature",
            "tool_choice",
            "tools",
            "top_p",
            "max_output_tokens",
            "instructions",
            "reasoning",
            "store",
            "text",
            "top_logprobs",
            "truncation",
            "user",
            "previous_response_id",
            "service_tier",
            "max_tool_calls",
            "prompt_cache_key",
            "safety_identifier",
        ],
        "Responses 响应",
    )?;
    let content = responses::parse_parts(
        map.get("output")
            .ok_or_else(|| TransformError("Responses 响应缺少 output".into()))?,
        reasoning_transport,
        terminal,
    )?;
    let stop = terminal.stop_reason(
        content
            .iter()
            .any(|part| matches!(part, ResponsePart::ToolCall { .. })),
    );
    Ok(CanonicalResponse {
        id: string(map.get("id"), "id")?,
        model: string(map.get("model"), "model")?,
        content,
        stop,
        usage: super::super::usage::parse(UpstreamProtocol::Responses, map.get("usage"))?,
    })
}

pub(super) fn parse_chat(
    value: &Value,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<CanonicalResponse, TransformError> {
    chat::parse(value, reasoning_transport)
}

pub(super) fn parse_anthropic(
    value: &Value,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<CanonicalResponse, TransformError> {
    anthropic::parse(value, reasoning_transport)
}
