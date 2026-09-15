use super::*;
use asb_core::contracts::{AppKind, UpstreamProtocol};
use asb_core::ownership::default_client_settings;
use tiny_http::{Header, StatusCode};

struct ClaudeFixture {
    _directory: tempfile::TempDir,
    gateway: GatewayController,
    token: String,
}

impl Drop for ClaudeFixture {
    fn drop(&mut self) {
        self.gateway.shutdown();
    }
}

fn fixture(first: &str, second: &str) -> ClaudeFixture {
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let first_profile = sandbox_profile(
        &local,
        AppKind::Claude,
        "Claude primary",
        first.to_string(),
        "primary-key".to_string(),
        UpstreamProtocol::ChatCompletions,
    );
    let _second_profile = sandbox_profile(
        &local,
        AppKind::Claude,
        "Claude fallback",
        second.to_string(),
        "fallback-key".to_string(),
        UpstreamProtocol::ChatCompletions,
    );
    let second_profile = local
        .configuration()
        .list_providers()
        .unwrap()
        .into_iter()
        .find(|record| record.profile.name == "Claude fallback")
        .expect("load Claude fallback profile")
        .profile;
    crate::gateway::failover::save(
        local.root(),
        &crate::gateway::failover::ClaudeFailoverPolicy {
            enabled: true,
            provider_ids: vec![first_profile.id.clone(), second_profile.id],
            max_retries: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let gateway = GatewayController::start(&local);
    let projection = gateway
        .project_with_candidates(
            &local,
            &SwitchPlan::direct(first_profile, default_client_settings(AppKind::Claude)),
        )
        .unwrap();
    let token = projection_token(&projection);
    gateway.commit(&projection, || Ok(())).unwrap();
    ClaudeFixture {
        _directory: directory,
        gateway,
        token,
    }
}

fn send_message(fixture: &ClaudeFixture, stream: bool) -> reqwest::blocking::Response {
    Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
        .post(format!(
            "{}/v1/messages",
            fixture.gateway.configured_base_url()
        ))
        .bearer_auth(&fixture.token)
        .header(CONTENT_TYPE, "application/json")
        .body(
            json!({
                "model": "sandbox-model",
                "max_tokens": 32,
                "stream": stream,
                "messages": [{"role": "user", "content": "hello"}]
            })
            .to_string(),
        )
        .send()
        .unwrap()
}

fn one_response(
    status: u16,
    body: &'static [u8],
    content_type_value: &'static str,
) -> (String, thread::JoinHandle<bool>) {
    let server = Server::http(("127.0.0.1", 0)).unwrap();
    let base = endpoint(&server);
    let worker = thread::spawn(move || {
        let Some(mut request) = server.recv_timeout(Duration::from_secs(5)).unwrap() else {
            return false;
        };
        let mut request_body = String::new();
        request
            .as_reader()
            .read_to_string(&mut request_body)
            .unwrap();
        assert!(request_body.contains("sandbox-model"));
        request
            .respond(
                Response::from_data(body.to_vec())
                    .with_status_code(StatusCode(status))
                    .with_header(content_type(content_type_value)),
            )
            .unwrap();
        true
    });
    (base, worker)
}

fn one_rate_limited_response(retry_after: &'static str) -> (String, thread::JoinHandle<bool>) {
    let server = Server::http(("127.0.0.1", 0)).unwrap();
    let base = endpoint(&server);
    let worker = thread::spawn(move || {
        let Some(mut request) = server.recv_timeout(Duration::from_secs(5)).unwrap() else {
            return false;
        };
        let mut request_body = String::new();
        request
            .as_reader()
            .read_to_string(&mut request_body)
            .unwrap();
        assert!(request_body.contains("sandbox-model"));
        request
            .respond(
                Response::from_string(r#"{"error":{"message":"rate limited"}}"#)
                    .with_status_code(StatusCode(429))
                    .with_header(content_type("application/json"))
                    .with_header(Header::from_bytes("retry-after", retry_after).unwrap()),
            )
            .unwrap();
        true
    });
    (base, worker)
}

#[test]
fn claude_retries_a_retryable_http_failure_on_the_next_provider() {
    let (first, first_worker) = one_response(
        503,
        br#"{"error":{"message":"primary unavailable"}}"#,
        "application/json",
    );
    let (second, second_worker) = one_response(
        200,
        br#"{"id":"chat_fallback","object":"chat.completion","model":"sandbox-model","choices":[{"index":0,"message":{"role":"assistant","content":"fallback ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#,
        "application/json",
    );
    let fixture = fixture(&first, &second);
    let response = send_message(&fixture, false);
    assert_eq!(response.status().as_u16(), 200);
    assert!(response.text().unwrap().contains("fallback ok"));
    assert!(first_worker.join().unwrap());
    assert!(second_worker.join().unwrap());
}

#[test]
fn claude_retries_a_success_status_error_envelope_before_replying() {
    let (first, first_worker) = one_response(
        200,
        br#"{"error":{"message":"upstream overloaded"}}"#,
        "application/json",
    );
    let (second, second_worker) = one_response(
        200,
        br#"{"id":"chat_semantic_fallback","object":"chat.completion","model":"sandbox-model","choices":[{"index":0,"message":{"role":"assistant","content":"semantic fallback ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#,
        "application/json",
    );
    let fixture = fixture(&first, &second);
    let response = send_message(&fixture, false);
    assert_eq!(response.status().as_u16(), 200);
    assert!(response.text().unwrap().contains("semantic fallback ok"));
    assert!(first_worker.join().unwrap());
    assert!(second_worker.join().unwrap());
}

#[test]
fn claude_does_not_fail_over_invalid_request_errors() {
    let (first, first_worker) = one_response(
        400,
        br#"{"error":{"message":"invalid request"}}"#,
        "application/json",
    );
    let (second, second_worker) = one_response(
        200,
        br#"{"id":"chat_unused","object":"chat.completion","model":"sandbox-model","choices":[{"index":0,"message":{"role":"assistant","content":"must not be used"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#,
        "application/json",
    );
    let fixture = fixture(&first, &second);
    let response = send_message(&fixture, false);
    assert_eq!(response.status().as_u16(), 400);
    assert!(response.text().unwrap().contains("invalid request"));
    assert!(first_worker.join().unwrap());
    assert!(!second_worker.join().unwrap());
}

#[test]
fn claude_streaming_response_stays_on_the_provider_after_the_first_chunk() {
    let (first, first_worker) = one_response(
        200,
        b"data: {\"id\":\"chat_stream\",\"object\":\"chat.completion.chunk\",\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_stream\",\"object\":\"chat.completion.chunk\",\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"stream ok\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_stream\",\"object\":\"chat.completion.chunk\",\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        "text/event-stream",
    );
    let (second, second_worker) = one_response(
        200,
        br#"{"id":"chat_unused","object":"chat.completion","model":"sandbox-model","choices":[{"index":0,"message":{"role":"assistant","content":"must not be used"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#,
        "application/json",
    );
    let fixture = fixture(&first, &second);
    let response = send_message(&fixture, true);
    assert_eq!(response.status().as_u16(), 200);
    let body = response.text().unwrap();
    assert!(body.contains("stream ok"), "stream body: {body}");
    assert!(first_worker.join().unwrap());
    assert!(!second_worker.join().unwrap());
}

#[test]
fn claude_retries_a_stream_that_ends_before_the_first_chunk() {
    let (first, first_worker) = one_response(200, b"", "text/event-stream");
    let (second, second_worker) = one_response(
        200,
        b"data: {\"id\":\"chat_stream\",\"object\":\"chat.completion.chunk\",\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"fallback stream\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        "text/event-stream",
    );
    let fixture = fixture(&first, &second);
    let response = send_message(&fixture, true);
    assert_eq!(response.status().as_u16(), 200);
    assert!(response.text().unwrap().contains("fallback stream"));
    assert!(first_worker.join().unwrap());
    assert!(second_worker.join().unwrap());
}

#[test]
fn claude_preserves_retry_after_when_all_candidates_are_rate_limited() {
    let (first, first_worker) = one_rate_limited_response("7");
    let (second, second_worker) = one_rate_limited_response("11");
    let fixture = fixture(&first, &second);
    let response = send_message(&fixture, false);
    assert_eq!(response.status().as_u16(), 429);
    assert_eq!(response.headers().get("retry-after").unwrap(), "11");
    assert!(response.text().unwrap().contains("rate limited"));
    assert!(first_worker.join().unwrap());
    assert!(second_worker.join().unwrap());
}

#[test]
fn claude_retries_an_upstream_error_in_the_initial_successful_sse_response() {
    let (first, first_worker) = one_response(
        200,
        b"data: {\"error\":{\"message\":\"temporarily unavailable\"}}\n\n",
        "text/event-stream",
    );
    let (second, second_worker) = one_response(200, b"data: {\"id\":\"ok\",\"object\":\"chat.completion.chunk\",\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"second upstream\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n", "text/event-stream");
    let fixture = fixture(&first, &second);
    let response = send_message(&fixture, true);
    assert_eq!(response.status().as_u16(), 200);
    assert!(response.text().unwrap().contains("second upstream"));
    assert!(first_worker.join().unwrap());
    assert!(second_worker.join().unwrap());
}

#[test]
fn claude_retries_unparseable_json_before_publishing_a_success_status() {
    let (first, first_worker) = one_response(200, b"not JSON", "application/json");
    let (second, second_worker) = one_response(200, br#"{"id":"ok","object":"chat.completion","model":"sandbox-model","choices":[{"index":0,"message":{"role":"assistant","content":"valid fallback"},"finish_reason":"stop"}]}"#, "application/json");
    let fixture = fixture(&first, &second);
    let response = send_message(&fixture, false);
    assert_eq!(response.status().as_u16(), 200);
    assert!(response.text().unwrap().contains("valid fallback"));
    assert!(first_worker.join().unwrap());
    assert!(second_worker.join().unwrap());
}

#[test]
fn claude_upstream_authentication_failures_try_only_the_explicit_fallback_queue() {
    for status in [401, 403] {
        let (first, first_worker) = one_response(
            status,
            br#"{"error":{"message":"upstream credentials unavailable"}}"#,
            "application/json",
        );
        let (second, second_worker) = one_response(200,
            br#"{"id":"chat_auth_fallback","object":"chat.completion","model":"sandbox-model","choices":[{"index":0,"message":{"role":"assistant","content":"explicit fallback"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#,
            "application/json");
        let fixture = fixture(&first, &second);
        let response = send_message(&fixture, false);
        assert_eq!(response.status().as_u16(), 200);
        assert!(response.text().unwrap().contains("explicit fallback"));
        assert!(first_worker.join().unwrap());
        assert!(second_worker.join().unwrap());
    }
}
