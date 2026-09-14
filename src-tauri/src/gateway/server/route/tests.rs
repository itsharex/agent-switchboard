use super::*;

const CAPABILITY: &str =
    "asb_codex_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn codex_paths_are_a_closed_typed_operation_set() {
    let cases = [
        ("responses", CodexOperation::Responses, Method::Post),
        ("responses/compact", CodexOperation::Compact, Method::Post),
        ("models", CodexOperation::Models, Method::Get),
        (
            "chat/completions",
            CodexOperation::ChatCompletions,
            Method::Post,
        ),
        ("alpha/search", CodexOperation::AlphaSearch, Method::Post),
        (
            "images/generations",
            CodexOperation::ImageGeneration,
            Method::Post,
        ),
        ("images/edits", CodexOperation::ImageEdit, Method::Post),
    ];
    for (path, expected, method) in cases {
        let url = format!("/codex/{CAPABILITY}/v1/{path}?trace=fixture");
        let request = codex_request(&url).expect("known Codex operation");
        assert_eq!(request.capability, CAPABILITY);
        assert_eq!(request.operation, expected);
        assert_eq!(request.operation.method(), method);
    }
    assert!(codex_request(&format!("/codex/{CAPABILITY}/v1/files")).is_none());
    assert!(codex_request(&format!("/codex/{CAPABILITY}/v1/v1/responses")).is_none());
    assert!(codex_request("/codex/asb_local_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/v1/responses").is_none());
}

#[test]
fn native_responses_headers_keep_protocol_facts_and_drop_credentials() {
    let incoming = [
        Header::from_bytes("openai-beta", "responses=v1").unwrap(),
        Header::from_bytes("user-agent", "codex/1.0").unwrap(),
        Header::from_bytes("session_id", "session-fixture").unwrap(),
        Header::from_bytes("x-stainless-lang", "rust").unwrap(),
        Header::from_bytes("Authorization", "Bearer official-token").unwrap(),
        Header::from_bytes("chatgpt-account-id", "official-account").unwrap(),
        Header::from_bytes("Cookie", "official_cookie=1").unwrap(),
        Header::from_bytes("x-request-id", "client-trace").unwrap(),
        Header::from_bytes("Content-Encoding", "zstd").unwrap(),
    ];
    let mut forwarded = HeaderMap::new();
    append_native_responses_headers(&mut forwarded, &incoming);
    assert_eq!(forwarded["openai-beta"], "responses=v1");
    assert_eq!(forwarded["user-agent"], "codex/1.0");
    assert_eq!(forwarded["session_id"], "session-fixture");
    assert_eq!(forwarded["x-stainless-lang"], "rust");
    for name in [
        "authorization",
        "chatgpt-account-id",
        "cookie",
        "x-request-id",
        "content-encoding",
    ] {
        assert!(
            !forwarded.contains_key(name),
            "{name} must not reach third-party upstreams"
        );
    }
}

#[test]
fn request_query_cannot_replace_the_fixed_upstream_target() {
    let upstream = "https://vendor.example/api/v1/responses".to_string();
    let gateway = "http://127.0.0.1:47821";
    let forwarded = with_request_query(
        upstream.clone(),
        gateway,
        "/codex/asb_codex_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/v1/responses?trace=a%20b&mode=test",
    )
    .expect("valid query");
    assert_eq!(
        forwarded,
        "https://vendor.example/api/v1/responses?trace=a%20b&mode=test"
    );

    let merged = with_request_query(
        "https://vendor.example/api/v1/responses?tenant=primary".to_string(),
        gateway,
        "/codex/token/v1/responses?trace=fixture",
    )
    .unwrap();
    assert_eq!(
        merged,
        "https://vendor.example/api/v1/responses?tenant=primary&trace=fixture"
    );

    for request_url in [
        "/codex/token/v1/responses?",
        "/codex/token/v1/responses?trace=1#other",
        "/codex/token/v1/responses?=value",
        "/codex/token/v1/responses?trace=1&",
        "/codex/token/v1/responses?trace=\nvalue",
    ] {
        assert!(
            with_request_query(upstream.clone(), gateway, request_url).is_err(),
            "unexpectedly accepted {request_url:?}"
        );
    }
}

#[test]
fn full_codex_urls_rewrite_known_sibling_operations_and_merge_queries() {
    let gateway = "http://127.0.0.1:47821";
    let cases = [
        (
            "https://relay.example/Gateway/v1/Responses/Compact/?api-version=2026-07",
            CodexOperation::AlphaSearch,
            "client_version=fixture",
            "https://relay.example/Gateway/v1/alpha/search?api-version=2026-07&client_version=fixture",
        ),
        (
            "https://relay.example/backend-api/codex/responses",
            CodexOperation::Compact,
            "trace=fixture",
            "https://relay.example/backend-api/codex/responses/compact?trace=fixture",
        ),
        (
            "https://relay.example/v1/chat/completions?tenant=one",
            CodexOperation::ImageGeneration,
            "trace=fixture",
            "https://relay.example/v1/images/generations?tenant=one&trace=fixture",
        ),
        (
            "https://relay.example/v1/images/generations",
            CodexOperation::ImageEdit,
            "trace=fixture",
            "https://relay.example/v1/images/edits?trace=fixture",
        ),
    ];

    for (base_url, operation, request_query, expected) in cases {
        let rewritten = rewrite_codex_full_url(base_url, operation.upstream_path())
            .expect("known full URL should be rewritable");
        let rewritten = with_request_query(
            rewritten,
            gateway,
            &format!(
                "/codex/asb_codex_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/v1/responses?{request_query}"
            ),
        )
        .expect("request query should merge");
        assert_eq!(rewritten, expected);
    }
}

#[test]
fn full_codex_urls_fail_closed_when_operation_sibling_is_opaque() {
    for operation in [
        CodexOperation::Compact,
        CodexOperation::AlphaSearch,
        CodexOperation::ImageGeneration,
        CodexOperation::ImageEdit,
    ] {
        let error = rewrite_codex_full_url(
            "https://relay.example/custom/rpc-endpoint?tenant=one",
            operation.upstream_path(),
        )
        .expect_err("opaque full URL must not be guessed");
        assert!(error.contains("无法从完整 Codex 地址推导"));
    }
}

#[test]
fn full_url_primary_requests_remain_exact_while_base_urls_append_paths() {
    let gateway = "http://127.0.0.1:47821";
    assert_eq!(
        upstream_url(
            &test_route(
                "https://relay.example/v1/responses?tenant=one",
                true,
                UpstreamProtocol::Responses,
            ),
            gateway,
            "/codex/token/v1/responses?trace=fixture",
        )
        .unwrap(),
        "https://relay.example/v1/responses?tenant=one&trace=fixture"
    );
    assert_eq!(
        upstream_url(
            &test_route(
                "https://relay.example/v1",
                false,
                UpstreamProtocol::Responses,
            ),
            gateway,
            "/codex/token/v1/responses",
        )
        .unwrap(),
        "https://relay.example/v1/responses"
    );
}

fn test_route(
    upstream_base_url: &str,
    is_full_url: bool,
    upstream_protocol: UpstreamProtocol,
) -> ActiveRoute {
    ActiveRoute {
        app: asb_core::contracts::AppKind::Codex,
        profile_id: "fixture".to_string(),
        fingerprint: "fixture-fingerprint".to_string(),
        client_token: CAPABILITY.to_string(),
        continuation_key: [0; 32],
        api_key: "fixture-key".to_string(),
        authentication: asb_core::AuthenticationScheme::Bearer,
        upstream_base_url: upstream_base_url.to_string(),
        upstream_protocol,
        connection: asb_core::contracts::ProviderConnectionOptions {
            is_full_url,
            ..Default::default()
        },
        responses_options: None,
        max_output_tokens: None,
        claude_primary_model: None,
        claude_model_options: None,
        claude_account: None,
        codex: None,
    }
}
