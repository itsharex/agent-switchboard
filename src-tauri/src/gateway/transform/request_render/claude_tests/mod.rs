//! Claude request compatibility, kept independent from Codex replay fixtures.

use crate::gateway::transform::{convert_request, ReasoningTransport, TransformError};
use asb_core::contracts::UpstreamProtocol;
use serde_json::{json, Value};

use super::tool_results::{TOOL_RESULT_ERROR_MARKER, TOOL_RESULT_MEDIA_MARKER};

const TARGETS: [UpstreamProtocol; 2] = [
    UpstreamProtocol::ChatCompletions,
    UpstreamProtocol::Responses,
];

fn request(model: &str) -> Value {
    json!({
        "model": model,
        "max_tokens": 64_000,
        "messages": [{ "role": "user", "content": "hello" }],
    })
}

fn convert(input: &Value, target: UpstreamProtocol) -> Result<Value, TransformError> {
    convert_with_transport(input, target, None)
}

fn convert_with_transport(
    input: &Value,
    target: UpstreamProtocol,
    transport: Option<&ReasoningTransport>,
) -> Result<Value, TransformError> {
    let converted = convert_request(
        UpstreamProtocol::AnthropicMessages,
        target,
        &serde_json::to_vec(input).expect("fixture JSON"),
        None,
        transport,
        None,
    )?;
    Ok(serde_json::from_slice(&converted.body).expect("converted JSON"))
}

fn image(data: &str) -> Value {
    json!({
        "type": "image",
        "source": { "type": "base64", "media_type": "image/png", "data": data },
    })
}

fn tool_call(id: &str) -> Value {
    json!({ "type": "tool_use", "id": id, "name": "capture", "input": {} })
}

fn output_effort(value: &Value, target: UpstreamProtocol) -> Option<&str> {
    match target {
        UpstreamProtocol::ChatCompletions => value.get("reasoning_effort"),
        UpstreamProtocol::Responses => value.pointer("/reasoning/effort"),
        _ => unreachable!(),
    }
    .and_then(Value::as_str)
}

mod cache;
mod codex;
mod efforts;
mod tool_results;

mod media;
