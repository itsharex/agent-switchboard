use super::*;
use tiny_http::{Header, StatusCode};
use tungstenite::client::IntoClientRequest;
use tungstenite::Message;

struct Fixture {
    _directory: tempfile::TempDir,
    gateway: GatewayController,
    token: String,
    endpoint: String,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.gateway.shutdown();
    }
}

fn minimal_gateway(upstream_base: &str) -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let mut file = sandbox_codex_file(
        &state,
        "Minimal sandbox",
        upstream_base.to_string(),
        "sandbox-upstream-secret".to_string(),
        CodexUpstream::Responses,
    );
    file.profile.endpoint = CodexEndpoint(format!("{upstream_base}/custom/api"));
    file.profile.default_model = "custom/model".to_string();
    file.profile.catalog[0].id = "custom/model".to_string();
    file.profile.model_routes[0].client_model = "custom/model".to_string();
    file.profile.model_routes[0].upstream_model = "custom/model".to_string();
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .unwrap();
    let rendered = asb_core::adapter::render("", &projection.plan).unwrap();
    assert!(rendered.contains("model_provider = \"openai\""));
    gateway.commit(&projection, || Ok(())).unwrap();
    Fixture {
        _directory: directory,
        gateway,
        token: projection_token(&projection),
        endpoint: format!("{upstream_base}/custom/api/responses"),
    }
}

fn upstream_once(status: u16, body: Vec<u8>, mime: &str) -> (String, thread::JoinHandle<Value>) {
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let base = endpoint(&upstream);
    let mime = mime.to_string();
    let worker = thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        assert_eq!(request.method(), &Method::Post);
        assert_eq!(request.url(), "/custom/api/responses");
        assert!(!request
            .headers()
            .iter()
            .any(|header| header.field.equiv("Upgrade")));
        let mut request_body = String::new();
        request
            .as_reader()
            .read_to_string(&mut request_body)
            .unwrap();
        let mut response = Response::from_data(body)
            .with_status_code(StatusCode(status))
            .with_header(content_type(&mime))
            .with_header(Header::from_bytes(b"x-request-id", b"req-sandbox-42").unwrap());
        if status == 429 {
            response.add_header(Header::from_bytes(b"retry-after", b"19").unwrap());
        }
        request.respond(response).unwrap();
        serde_json::from_str(&request_body).unwrap()
    });
    (base, worker)
}

fn request_body(stream: bool) -> Value {
    json!({"model":"custom/model", "input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}],
        "stream":stream, "reasoning":{"effort":"high"}, "service_tier":"priority", "store":false,
        "include":["reasoning.encrypted_content"], "metadata":{"session":"local"},
        "tools":[{"type":"function","name":"write","parameters":{"type":"object"}}],
        "tool_choice":{"type":"function","name":"write"}, "parallel_tool_calls":false})
}

fn http_request(fixture: &Fixture, stream: bool) -> reqwest::blocking::Response {
    http_request_with_headers(fixture, stream, &[])
}

fn http_request_with_headers(
    fixture: &Fixture,
    stream: bool,
    headers: &[(&str, &str)],
) -> reqwest::blocking::Response {
    let mut request = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
        .post(format!(
            "{}/codex/{}/v1/responses",
            fixture.gateway.configured_base_url(),
            fixture.token
        ))
        .bearer_auth("fake-official-oauth")
        .header(CONTENT_TYPE, "application/json")
        .body(request_body(stream).to_string());
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    request.send().unwrap()
}

fn assert_native_responses(body: &Value) {
    assert_eq!(body["model"], "custom/model");
    assert_eq!(body["tools"][0]["name"], "write");
    assert_eq!(body["tool_choice"]["name"], "write");
    assert_eq!(body["parallel_tool_calls"], false);
    assert_eq!(body["reasoning"]["effort"], "high");
    assert_eq!(body["service_tier"], "priority");
    assert_eq!(body["store"], false);
    assert_eq!(body["include"][0], "reasoning.encrypted_content");
    assert_eq!(body["metadata"]["session"], "local");
}

#[test]
fn http_failure_keeps_upstream_facts_and_redacts_only_credentials() {
    let error = json!({"error":{"code":"invalid_request_error", "message":"custom/model rejects reasoning; sandbox-upstream-secret", "api_key":"echoed-secret"}});
    let (base, worker) = upstream_once(400, error.to_string().into_bytes(), "application/json");
    let fixture = minimal_gateway(&base);
    let response = http_request(&fixture, false);
    assert_eq!(response.status().as_u16(), 400);
    assert_eq!(response.headers()["x-request-id"], "req-sandbox-42");
    let body = response.text().unwrap();
    let value: Value = serde_json::from_str(&body).unwrap();
    let diagnostic = &value["error"]["diagnostic"];
    assert_eq!(diagnostic["kind"], "requestParameters");
    assert_eq!(diagnostic["endpoint"], fixture.endpoint);
    assert_eq!(diagnostic["status"], 400);
    assert_eq!(diagnostic["requestId"], "req-sandbox-42");
    assert!(body.contains("custom/model"));
    assert!(!body.contains("sandbox-upstream-secret"));
    assert!(!body.contains("echoed-secret"));
    assert_native_responses(&worker.join().unwrap());
}

#[test]
fn non_utf8_http_400_is_not_treated_as_a_stream_parse_failure() {
    let (base, worker) = upstream_once(
        400,
        b"invalid model parameter: \xff".to_vec(),
        "text/event-stream",
    );
    let fixture = minimal_gateway(&base);
    let response = http_request(&fixture, true);
    assert_eq!(response.status().as_u16(), 400);
    let value: Value = serde_json::from_str(&response.text().unwrap()).unwrap();
    assert_eq!(value["error"]["diagnostic"]["kind"], "requestParameters");
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("invalid model parameter"));
    worker.join().unwrap();
}

#[test]
fn compressed_native_responses_body_is_decoded_before_client_delivery() {
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let fixture = minimal_gateway(&endpoint(&upstream));
    let worker = thread::spawn(move || {
        let request = upstream
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        assert_eq!(request.method(), &Method::Post);
        assert!(request.headers().iter().any(|header| {
            header.field.equiv("Accept-Encoding") && header.value.as_str() == "identity"
        }));
        let body = json!({
            "id": "resp_compressed",
            "object": "response",
            "status": "completed",
            "model": "sandbox-model",
            "output": [{
                "type": "message",
                "id": "msg_compressed",
                "status": "completed",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "decoded response",
                    "annotations": []
                }]
            }],
            "usage": { "input_tokens": 3, "output_tokens": 4, "total_tokens": 7 },
            "error": null
        });
        let encoded = zstd::stream::encode_all(body.to_string().as_bytes(), 0).unwrap();
        request
            .respond(
                Response::from_data(encoded)
                    .with_header(content_type("application/json"))
                    .with_header(Header::from_bytes(b"Content-Encoding", b"zstd").unwrap()),
            )
            .unwrap();
    });

    let response = http_request(&fixture, false);
    assert_eq!(response.status().as_u16(), 200);
    let body: Value = serde_json::from_str(&response.text().unwrap()).unwrap();
    assert_eq!(body["id"], "resp_compressed");
    assert_eq!(body["output"][0]["content"][0]["text"], "decoded response");
    worker.join().unwrap();
}

#[test]
fn native_responses_success_preserves_status_and_allowed_headers() {
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let fixture = minimal_gateway(&endpoint(&upstream));
    let worker = thread::spawn(move || {
        let request = upstream
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        request
            .respond(
                Response::from_string(
                    json!({
                        "id": "resp_created",
                        "object": "response",
                        "status": "completed",
                        "model": "custom/model",
                        "output": [],
                        "error": null
                    })
                    .to_string(),
                )
                .with_status_code(StatusCode(201))
                .with_header(content_type("application/json"))
                .with_header(Header::from_bytes(b"x-request-id", b"req-created-42").unwrap())
                .with_header(Header::from_bytes(b"x-ratelimit-remaining-requests", b"17").unwrap())
                .with_header(Header::from_bytes(b"set-cookie", b"upstream=session").unwrap()),
            )
            .unwrap();
    });

    let response = http_request(&fixture, false);
    assert_eq!(response.status().as_u16(), 201);
    assert_eq!(response.headers()["x-request-id"], "req-created-42");
    assert_eq!(response.headers()["x-ratelimit-remaining-requests"], "17");
    assert!(response.headers().get("set-cookie").is_none());
    assert_eq!(
        serde_json::from_str::<Value>(&response.text().unwrap()).unwrap()["id"],
        "resp_created"
    );
    worker.join().unwrap();
}

#[test]
fn native_responses_forwards_protocol_headers_but_not_official_identity() {
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let fixture = minimal_gateway(&endpoint(&upstream));
    let worker = thread::spawn(move || {
        let request = upstream
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        let header = |name: &str| {
            request
                .headers()
                .iter()
                .find(|header| header.field.as_str().as_str().eq_ignore_ascii_case(name))
                .map(|header| header.value.as_str())
        };
        assert_eq!(header("openai-beta"), Some("responses=v1"));
        assert_eq!(header("user-agent"), Some("codex/fixture"));
        assert_eq!(header("session_id"), Some("session-fixture"));
        assert_eq!(header("x-stainless-lang"), Some("rust"));
        assert_eq!(
            header("authorization"),
            Some("Bearer sandbox-upstream-secret")
        );
        for name in ["chatgpt-account-id", "cookie", "x-request-id"] {
            assert!(header(name).is_none(), "{name} must not reach upstream");
        }
        request
            .respond(
                Response::from_string(
                    json!({
                        "id": "resp_headers",
                        "object": "response",
                        "status": "completed",
                        "model": "custom/model",
                        "output": [],
                        "error": null
                    })
                    .to_string(),
                )
                .with_header(content_type("application/json")),
            )
            .unwrap();
    });
    let response = http_request_with_headers(
        &fixture,
        false,
        &[
            ("openai-beta", "responses=v1"),
            ("user-agent", "codex/fixture"),
            ("session_id", "session-fixture"),
            ("x-stainless-lang", "rust"),
            ("chatgpt-account-id", "official-account"),
            ("cookie", "official_cookie=1"),
            ("x-request-id", "client-trace"),
        ],
    );
    assert_eq!(response.status().as_u16(), 200);
    worker.join().unwrap();
}

#[test]
fn malformed_sse_finishes_with_a_diagnostic_event_instead_of_disconnect() {
    let (base, worker) = upstream_once(
        200,
        b"event: response.output_text.delta\ndata: \xff\n\n".to_vec(),
        "text/event-stream",
    );
    let fixture = minimal_gateway(&base);
    let response = http_request(&fixture, true);
    assert_eq!(response.status().as_u16(), 200);
    let body = response
        .text()
        .expect("HTTP chunked stream must terminate correctly");
    assert!(body.contains("event: response.failed"));
    assert!(body.contains("\"kind\":\"streamParse\""));
    assert!(body.contains("req-sandbox-42"));
    assert!(body.contains(&fixture.endpoint));
    assert!(body.contains("UTF-8"));
    worker.join().unwrap();
}

#[test]
fn local_websocket_uses_http_upstream_and_preserves_the_same_failure() {
    let (base, worker) = upstream_once(
        404,
        br#"{"error":{"code":"model_not_found","message":"custom/model does not exist"}}"#.to_vec(),
        "application/json",
    );
    let fixture = minimal_gateway(&base);
    let address = fixture.gateway.configured_base_url();
    let mut request = format!(
        "{}/codex/{}/v1/responses",
        address.replace("http://", "ws://"),
        fixture.token
    )
    .into_client_request()
    .unwrap();
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", fixture.token).parse().unwrap(),
    );
    let tcp = TcpStream::connect(address.strip_prefix("http://").unwrap()).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let (mut socket, _) = tungstenite::client(request, tcp).unwrap();
    let mut body = request_body(true);
    body["type"] = json!("response.create");
    socket.send(Message::Text(body.to_string())).unwrap();
    let frame = socket.read().unwrap().into_text().unwrap();
    let value: Value = serde_json::from_str(&frame).unwrap();
    assert_eq!(value["type"], "response.failed");
    let diagnostic = &value["response"]["error"]["diagnostic"];
    assert_eq!(diagnostic["status"], 404);
    assert_eq!(diagnostic["kind"], "modelNotFound");
    assert_eq!(diagnostic["transport"], "http");
    assert_eq!(diagnostic["endpoint"], fixture.endpoint);
    assert_eq!(diagnostic["requestId"], "req-sandbox-42");
    socket.close(None).unwrap();
    assert_native_responses(&worker.join().unwrap());
}

#[test]
fn successful_http_headers_are_redacted_when_json_or_sse_parsing_fails() {
    for stream in [false, true] {
        let upstream = Server::http(("127.0.0.1", 0)).unwrap();
        let fixture = minimal_gateway(&endpoint(&upstream));
        let worker = thread::spawn(move || {
            let request = upstream
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .unwrap();
            request
                .respond(
                    Response::from_data(if stream {
                        b"data: invalid\n\n".to_vec()
                    } else {
                        b"invalid".to_vec()
                    })
                    .with_header(content_type(if stream {
                        "text/event-stream"
                    } else {
                        "application/json"
                    }))
                    .with_header(
                        Header::from_bytes(b"x-request-id", b"req-sandbox-upstream-secret")
                            .unwrap(),
                    ),
                )
                .unwrap();
        });
        let response = http_request(&fixture, stream);
        assert_eq!(response.status().as_u16(), if stream { 200 } else { 502 });
        for header in response.headers().values() {
            assert!(!String::from_utf8_lossy(header.as_bytes()).contains("sandbox-upstream-secret"));
        }
        let text = response.text().unwrap();
        assert!(!text.contains("sandbox-upstream-secret"));
        assert!(text.contains(&format!("req-{}", asb_core::redact::REDACTED)));
        assert!(text.contains(&fixture.endpoint));
        worker.join().unwrap();
    }
}

#[test]
fn upstream_401_is_an_upstream_credential_error_without_official_login_challenge() {
    let (base, worker) = upstream_once(
        401,
        br#"{"error":{"message":"invalid vendor key"}}"#.to_vec(),
        "application/json",
    );
    let fixture = minimal_gateway(&base);
    let response = http_request(&fixture, false);
    assert_eq!(response.status().as_u16(), 502);
    let body: Value = serde_json::from_str(&response.text().unwrap()).unwrap();
    assert_eq!(body["error"]["code"], "upstream_authentication_failed");
    assert_eq!(body["error"]["upstreamStatus"], 401);
    assert_eq!(body["error"]["diagnostic"]["status"], 401);
    worker.join().unwrap();
}

#[test]
fn upstream_rate_limit_preserves_retry_after_through_the_error_envelope() {
    let (base, worker) = upstream_once(
        429,
        br#"{"error":{"message":"rate limited"}}"#.to_vec(),
        "application/json",
    );
    let fixture = minimal_gateway(&base);
    let response = http_request(&fixture, false);
    assert_eq!(response.status().as_u16(), 429);
    assert_eq!(response.headers()["retry-after"], "19");
    let body: Value = serde_json::from_str(&response.text().unwrap()).unwrap();
    assert_eq!(body["error"]["diagnostic"]["retryAfter"], "19");
    worker.join().unwrap();
}
