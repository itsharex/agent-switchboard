use super::*;
use crate::provider_diagnostics::{ProviderDiagnostic, ProviderFailureKind};
use serde_json::json;

fn context() -> ProviderDiagnostic {
    let mut diagnostic = ProviderDiagnostic::new(
        ProviderFailureKind::StreamParse,
        "https://provider.example/custom/responses",
        "",
    );
    diagnostic.status = Some(200);
    diagnostic.request_id = Some("req-stream-42".to_string());
    diagnostic
}

fn stream_text(source: &[u8], from: UpstreamProtocol, to: UpstreamProtocol) -> String {
    let mut stream = SseTranscoder::new(source, from, to, 8192, None)
        .unwrap()
        .with_diagnostics(context(), &["sandbox-secret"]);
    let mut output = String::new();
    stream.read_to_string(&mut output).unwrap();
    output
}

#[test]
fn native_responses_stream_requires_a_terminal_event() {
    let body = b"event: response.created\ndata: {\"type\":\"response.created\"}\n\n";
    let text = stream_text(
        body,
        UpstreamProtocol::Responses,
        UpstreamProtocol::Responses,
    );
    assert!(text.contains("response.created"));
    assert!(text.contains("response.failed"));
    assert!(text.contains("终止事件"));
    assert!(text.contains("req-stream-42"));
}

#[test]
fn provider_sse_error_retains_body_and_is_not_a_parse_error() {
    let error = json!({"type":"response.failed","response":{"error":{"code":"invalid_request_error","message":"invalid field reasoning sandbox-secret"}}});
    let body = format!("event: response.failed\ndata: {error}\n\n");
    let text = stream_text(
        body.as_bytes(),
        UpstreamProtocol::Responses,
        UpstreamProtocol::AnthropicMessages,
    );
    assert!(text.contains("\"kind\":\"upstream\""));
    assert!(text.contains("invalid field reasoning"));
    assert!(text.contains("req-stream-42"));
    assert!(!text.contains("sandbox-secret"));
}

#[test]
fn broken_source_emits_a_network_error_with_endpoint() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "connection reset",
            ))
        }
    }
    let mut stream = SseTranscoder::new(
        Broken,
        UpstreamProtocol::Responses,
        UpstreamProtocol::Responses,
        8192,
        None,
    )
    .unwrap()
    .with_diagnostics(context(), &[]);
    let mut text = String::new();
    stream.read_to_string(&mut text).unwrap();
    assert!(stream.failed());
    assert!(text.contains("\"kind\":\"network\""));
    assert!(text.contains("https://provider.example/custom/responses"));
}
