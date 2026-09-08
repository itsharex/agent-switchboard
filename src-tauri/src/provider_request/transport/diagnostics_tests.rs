use super::tests::{execute_at, fixture, http};
use super::*;
use crate::provider_request::tests::{direct_client, record, success_body};

fn error_response(status: u16, body: &[u8], declared_length: usize) -> Vec<u8> {
    let mut raw = format!(
        "HTTP/1.1 {status} Fixture\r\nX-Request-Id: upstream-request-12345678901234567890\r\nContent-Type: application/json\r\nContent-Length: {declared_length}\r\nConnection: close\r\n\r\n"
    ).into_bytes();
    raw.extend_from_slice(body);
    raw
}

#[test]
fn http_errors_keep_status_endpoint_request_id_and_non_utf8_body() {
    let body = b"unsupported reasoning parameter: \xff";
    let (endpoint, server) = fixture(error_response(400, body, body.len()));
    let result = execute_at(UpstreamProtocol::Responses, endpoint.clone());
    assert_eq!(result.outcome, Outcome::HttpError);
    assert_eq!(result.status, Some(400));
    let diagnostic = result.diagnostic.unwrap();
    assert_eq!(diagnostic.kind, ProviderFailureKind::RequestParameters);
    assert_eq!(diagnostic.endpoint, endpoint);
    assert_eq!(
        diagnostic.request_id.as_deref(),
        Some("upstream-request-12345678901234567890")
    );
    assert_eq!(
        diagnostic.body.as_deref(),
        Some("unsupported reasoning parameter: �")
    );
    assert!(!diagnostic.body_truncated);
    let request = server.join().unwrap().to_ascii_lowercase();
    assert!(!request.contains("upgrade:"));
}

#[test]
fn incomplete_error_body_still_reports_the_upstream_http_failure() {
    let body = b"{\"error\":\"unsupported service_tier\"}";
    let (endpoint, server) = fixture(error_response(400, body, body.len() + 128));
    let result = execute_at(UpstreamProtocol::Responses, endpoint.clone());
    assert_eq!(result.outcome, Outcome::HttpError);
    assert_eq!(result.status, Some(400));
    let diagnostic = result.diagnostic.unwrap();
    assert_eq!(diagnostic.kind, ProviderFailureKind::RequestParameters);
    assert_eq!(diagnostic.endpoint, endpoint);
    assert!(diagnostic.request_id.is_some());
    assert!(diagnostic.body_truncated);
    assert!(diagnostic.message.contains("读取上游错误响应"));
    server.join().unwrap();
}

#[test]
fn an_endpoint_error_and_a_missing_model_have_distinct_diagnostics() {
    for (body, kind) in [
        ("<html>Not Found</html>", ProviderFailureKind::Endpoint),
        (
            r#"{"error":{"code":"model_not_found","message":"model does not exist"}}"#,
            ProviderFailureKind::ModelNotFound,
        ),
    ] {
        let (endpoint, server) = fixture(http(404, body));
        let result = execute_at(UpstreamProtocol::Responses, endpoint);
        assert_eq!(result.status, Some(404));
        let diagnostic = result.diagnostic.unwrap();
        assert_eq!(diagnostic.kind, kind);
        assert!(diagnostic
            .body
            .unwrap()
            .contains(if kind == ProviderFailureKind::Endpoint {
                "Not Found"
            } else {
                "model_not_found"
            }));
        server.join().unwrap();
    }
}

#[test]
fn oversized_http_error_remains_an_http_error_with_an_explicit_body_limit() {
    let body = "x".repeat(MAX_DIAGNOSTIC_BODY_BYTES + 1_024);
    let (endpoint, server) = fixture(http(400, &body));
    let result = execute_at(UpstreamProtocol::Responses, endpoint);
    assert_eq!(result.outcome, Outcome::HttpError);
    assert_eq!(result.status, Some(400));
    let diagnostic = result.diagnostic.unwrap();
    assert!(diagnostic.body_truncated);
    assert!(diagnostic.body.unwrap().len() <= MAX_DIAGNOSTIC_BODY_BYTES);
    server.join().unwrap();
}

#[test]
fn a_success_status_with_html_reports_response_parsing_and_retains_the_body() {
    let (endpoint, server) = fixture(http(200, "<html>Reverse proxy landing page</html>"));
    let result = execute_at(UpstreamProtocol::Responses, endpoint.clone());
    assert_eq!(result.outcome, Outcome::InvalidResponse);
    let diagnostic = result.diagnostic.unwrap();
    assert_eq!(diagnostic.kind, ProviderFailureKind::ResponseParse);
    assert_eq!(diagnostic.endpoint, endpoint);
    assert_eq!(diagnostic.status, Some(200));
    assert!(diagnostic
        .body
        .unwrap()
        .contains("Reverse proxy landing page"));
    server.join().unwrap();
}

#[test]
fn configured_minimal_requests_omit_optional_store_and_keep_input_and_model() {
    let (endpoint, server) = fixture(http(
        200,
        &success_body(UpstreamProtocol::Responses, "ok").to_string(),
    ));
    let mut profile = record(UpstreamProtocol::Responses).profile;
    profile.responses_options.as_mut().unwrap().request_mode = ResponsesRequestMode::Minimal;
    let result = tauri::async_runtime::block_on(send(
        direct_client(),
        crate::provider_request::ProviderRequestConnection::try_from(&profile).unwrap(),
        endpoint,
        "requested-model".into(),
        Instant::now(),
    ))
    .unwrap();
    assert_eq!(result.outcome, Outcome::Success);
    let raw = server.join().unwrap();
    let (headers, body) = raw.split_once("\r\n\r\n").unwrap();
    assert!(!headers.to_ascii_lowercase().contains("upgrade:"));
    let body: Value = serde_json::from_str(body).unwrap();
    assert_eq!(body["model"], "requested-model");
    assert_eq!(body["stream"], false);
    assert_eq!(body["max_output_tokens"], MAX_OUTPUT_TOKENS);
    assert!(body.get("store").is_none());
    assert!(body["input"].is_array());
}
