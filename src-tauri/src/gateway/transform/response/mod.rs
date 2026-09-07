use super::tool_names::{parse_target_name, render_target_name};
use super::{
    error, CanonicalResponse, Reasoning, ReasoningTransport, ResponsePart, StopReason,
    TransformError, Usage,
};
use asb_core::contracts::UpstreamProtocol;
use serde_json::{json, Map, Value};

pub(crate) fn convert_response(
    from: UpstreamProtocol,
    to: UpstreamProtocol,
    body: &[u8],
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<Vec<u8>, TransformError> {
    if from == to {
        return Ok(body.to_vec());
    }
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| TransformError("上游响应不是有效 JSON".to_string()))?;
    let mut response = parse_response(from, &value, reasoning_transport)?;
    if to == UpstreamProtocol::Responses {
        decode_target_tool_names(&mut response)?;
    }
    let value = render_response(to, &response)?;
    serde_json::to_vec(&value).map_err(|_| TransformError("无法编码转换后的响应".to_string()))
}

fn decode_target_tool_names(response: &mut CanonicalResponse) -> Result<(), TransformError> {
    for part in &mut response.content {
        let ResponsePart::ToolCall {
            name, namespace, ..
        } = part
        else {
            continue;
        };
        let (decoded_namespace, decoded_name) = parse_target_name(name)?;
        *namespace = decoded_namespace;
        *name = decoded_name;
    }
    Ok(())
}

pub(super) fn parse_response(
    protocol: UpstreamProtocol,
    value: &Value,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<CanonicalResponse, TransformError> {
    match protocol {
        UpstreamProtocol::Responses => parse_responses(value, reasoning_transport),
        UpstreamProtocol::ChatCompletions => parse_chat(value, reasoning_transport),
        UpstreamProtocol::AnthropicMessages => parse_anthropic(value, reasoning_transport),
    }
}

pub(super) fn render_response(
    protocol: UpstreamProtocol,
    response: &CanonicalResponse,
) -> Result<Value, TransformError> {
    match protocol {
        UpstreamProtocol::Responses => render_responses(response),
        UpstreamProtocol::ChatCompletions => render_chat(response),
        UpstreamProtocol::AnthropicMessages => render_anthropic(response),
    }
}

/// Generates the selected client protocol's normal error envelope. The error
/// text is intentionally gateway-owned and never includes an upstream URL,
/// header, body, or credential.
pub(crate) fn convert_error(to: UpstreamProtocol, status: u16, message: &str) -> Vec<u8> {
    let value = match to {
        UpstreamProtocol::AnthropicMessages => json!({
            "type": "error",
            "error": { "type": "api_error", "message": message },
        }),
        UpstreamProtocol::Responses => json!({
            "object": "error",
            "status": status,
            "error": { "type": "gateway_error", "message": message },
        }),
        UpstreamProtocol::ChatCompletions => json!({
            "error": { "message": message, "type": "gateway_error", "code": status.to_string() },
        }),
    };
    serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec())
}

mod parse;
mod render;

#[cfg(test)]
mod tests;

pub(super) use render::responses_reasoning_item;

use parse::*;
use render::*;
