use super::*;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::json;

#[test]
fn http_errors_preserve_status_endpoint_id_body_and_specific_kind() {
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", HeaderValue::from_static("req-provider-123"));
    let cases = [
        (
            400,
            r#"{"error":{"message":"reasoning is unsupported"}}"#,
            ProviderFailureKind::RequestParameters,
        ),
        (
            404,
            r#"{"error":{"code":"model_not_found","message":"unknown model custom/m"}}"#,
            ProviderFailureKind::ModelNotFound,
        ),
        (
            404,
            "<html>route does not exist</html>",
            ProviderFailureKind::Endpoint,
        ),
        (
            401,
            "authentication denied",
            ProviderFailureKind::Authentication,
        ),
        (429, "retry later", ProviderFailureKind::RateLimit),
        (503, "backend unavailable", ProviderFailureKind::Upstream),
    ];
    for (status, body, kind) in cases {
        let diagnostic = http_diagnostic(
            "https://provider.example/api/responses",
            status,
            &headers,
            body.as_bytes(),
            false,
            &[],
        );
        assert_eq!(diagnostic.kind, kind);
        assert_eq!(diagnostic.status, Some(status));
        assert_eq!(diagnostic.request_id.as_deref(), Some("req-provider-123"));
        assert!(diagnostic
            .summary()
            .contains("https://provider.example/api/responses"));
        assert!(diagnostic.summary().contains(body));
    }
}

#[test]
fn diagnostics_redact_credentials_but_keep_model_endpoint_and_request_id() {
    let body = json!({"error": {
        "message": "model custom/vendor-model at https://relay.example/custom/responses rejects sk-sandbox-secret",
        "headers": { "Authorization": "Bearer another-secret", "x-api-key": "second-secret" },
        "api_key": "third-secret", "request_id": "req-real-123", "model": "custom/vendor-model"
    }});
    let diagnostic = http_diagnostic(
        "https://relay.example/custom/responses",
        400,
        &HeaderMap::new(),
        body.to_string().as_bytes(),
        false,
        &["sk-sandbox-secret"],
    );
    let summary = diagnostic.summary();
    for secret in [
        "sk-sandbox-secret",
        "another-secret",
        "second-secret",
        "third-secret",
    ] {
        assert!(!summary.contains(secret), "{secret}");
    }
    assert!(summary.contains("custom/vendor-model"));
    assert!(summary.contains("req-real-123"));
    assert!(summary.contains("https://relay.example/custom/responses"));
}

#[test]
fn truncated_body_does_not_expose_a_partial_raw_or_json_escaped_key() {
    for secret in ["sk-sandbox-secret", "sk-quoted-\"secret"] {
        let encoded = serde_json::to_string(secret).unwrap();
        for variant in [secret, &encoded[1..encoded.len() - 1]] {
            let prefix = &variant[..variant.len() - 2];
            let body = format!(
                "{}{}",
                "x".repeat(MAX_DIAGNOSTIC_BODY_BYTES - prefix.len()),
                prefix
            );
            let diagnostic = http_diagnostic(
                "https://relay.example/responses",
                400,
                &HeaderMap::new(),
                body.as_bytes(),
                true,
                &[secret],
            );
            assert!(diagnostic.body_truncated);
            assert!(!diagnostic.body.unwrap().contains(prefix));
        }
    }
}

#[test]
fn non_utf8_error_body_keeps_http_failure_instead_of_becoming_parse_error() {
    let diagnostic = http_diagnostic(
        "https://relay.example/responses",
        400,
        &HeaderMap::new(),
        b"bad input: \xff",
        false,
        &[],
    );
    assert_eq!(diagnostic.kind, ProviderFailureKind::RequestParameters);
    assert!(diagnostic.body.unwrap().contains("bad input:"));
}

#[test]
fn transport_classes_are_distinct() {
    for (message, expected) in [
        ("dns error: no such host", ProviderFailureKind::Dns),
        (
            "invalid peer certificate: unknown issuer",
            ProviderFailureKind::Tls,
        ),
        ("operation timed out", ProviderFailureKind::Timeout),
        ("connection refused", ProviderFailureKind::Network),
    ] {
        assert_eq!(
            network_failure_kind(&std::io::Error::other(message)),
            expected
        );
    }
}
