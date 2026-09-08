//! Shared serialization for gateway-generated Server-Sent Events.
//!
//! Streaming conversion owns parsing and state in `stream.rs`; this module
//! owns only the target-event wire framing so there is no second buffered
//! conversion implementation to keep in sync.

use crate::provider_diagnostics::ProviderDiagnostic;
use asb_core::contracts::UpstreamProtocol;
use serde_json::Value;

pub(crate) fn diagnostic_event(
    protocol: UpstreamProtocol,
    diagnostic: &ProviderDiagnostic,
) -> Vec<u8> {
    let mut error = diagnostic.error_value();
    let (event, value) = match protocol {
        UpstreamProtocol::Responses => (
            "response.failed",
            serde_json::json!({
                "type":"response.failed", "response":{"object":"response", "status":"failed", "error":error}
            }),
        ),
        UpstreamProtocol::AnthropicMessages => {
            error["type"] = serde_json::json!("api_error");
            ("error", serde_json::json!({"type":"error", "error":error}))
        }
        UpstreamProtocol::ChatCompletions => ("error", serde_json::json!({"error":error})),
    };
    render_event(event, &value)
}

pub(super) fn render_event(event: &str, value: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    output.extend_from_slice(b"event: ");
    output.extend_from_slice(event.as_bytes());
    output.extend_from_slice(b"\n");
    output.extend_from_slice(b"data: ");
    let json = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    output.extend_from_slice(json.as_bytes());
    output.extend_from_slice(b"\n\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn emits_one_complete_sse_event() {
        let output = String::from_utf8(render_event("response.created", &json!({ "ok": true })))
            .expect("event must be UTF-8");
        assert_eq!(output, "event: response.created\ndata: {\"ok\":true}\n\n");
    }
}
