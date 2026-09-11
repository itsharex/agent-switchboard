use super::*;
use std::io::Write;
use tiny_http::Header;
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

fn native_responses_gateway(upstream_base: &str) -> Fixture {
    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let mut file = sandbox_codex_file(
        &state,
        "content encoding sandbox",
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
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("activate route");
    Fixture {
        _directory: directory,
        gateway,
        token: projection_token(&projection),
        endpoint: format!("{upstream_base}/custom/api/responses"),
    }
}

fn request_body(stream: bool) -> Value {
    json!({
        "model": "custom/model",
        "input": [{
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": "hello"}],
        }],
        "stream": stream,
        "store": false,
    })
}

fn http_request(fixture: &Fixture) -> reqwest::blocking::Response {
    Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("HTTP client")
        .post(format!(
            "{}/codex/{}/v1/responses",
            fixture.gateway.configured_base_url(),
            fixture.token
        ))
        .header(CONTENT_TYPE, "application/json")
        .body(request_body(true).to_string())
        .send()
        .expect("gateway request")
}

fn gzip(payload: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(payload).expect("compress payload");
    encoder.finish().expect("finish gzip")
}

fn completed_response(id: &str) -> Value {
    json!({
        "id": id,
        "object": "response",
        "status": "completed",
        "model": "custom/model",
        "output": [{
            "type": "message",
            "id": format!("msg_{id}"),
            "status": "completed",
            "role": "assistant",
            "content": [{"type": "output_text", "text": "decoded stream", "annotations": []}],
        }],
        "usage": {"input_tokens": 1, "output_tokens": 2, "total_tokens": 3},
        "error": null,
    })
}

#[test]
fn compressed_sse_decode_failure_emits_a_stream_parse_diagnostic() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let fixture = native_responses_gateway(&endpoint(&upstream));
    let worker = thread::spawn(move || {
        let request = upstream
            .recv_timeout(Duration::from_secs(10))
            .expect("upstream receive")
            .expect("upstream request");
        request
            .respond(
                Response::from_data(b"not a gzip stream".to_vec())
                    .with_header(content_type("text/event-stream"))
                    .with_header(Header::from_bytes("Content-Encoding", "gzip").unwrap())
                    .with_header(Header::from_bytes("x-request-id", "req-gzip-invalid").unwrap()),
            )
            .expect("upstream response");
    });

    let response = http_request(&fixture);
    assert_eq!(response.status().as_u16(), 200);
    let body = response.text().expect("gateway SSE body");
    assert!(body.contains("event: response.failed"));
    assert!(body.contains("\"kind\":\"streamParse\""));
    assert!(body.contains("无法解压上游 SSE"));
    assert!(body.contains("req-gzip-invalid"));
    assert!(body.contains(&fixture.endpoint));
    worker.join().expect("upstream worker");
}

#[test]
fn websocket_decodes_a_gzip_native_responses_stream_over_http_upstream() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let fixture = native_responses_gateway(&endpoint(&upstream));
    let worker = thread::spawn(move || {
        let request = upstream
            .recv_timeout(Duration::from_secs(10))
            .expect("upstream receive")
            .expect("upstream request");
        let event = json!({
            "type": "response.completed",
            "response": completed_response("resp_ws_gzip"),
        });
        request
            .respond(
                Response::from_data(gzip(
                    format!("event: response.completed\ndata: {event}\n\n").as_bytes(),
                ))
                .with_header(content_type("text/event-stream"))
                .with_header(Header::from_bytes("Content-Encoding", "gzip").unwrap()),
            )
            .expect("upstream response");
    });

    let address = fixture.gateway.configured_base_url();
    let mut request = format!(
        "{}/codex/{}/v1/responses",
        address.replace("http://", "ws://"),
        fixture.token
    )
    .into_client_request()
    .expect("WebSocket request");
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", fixture.token).parse().unwrap(),
    );
    let tcp = TcpStream::connect(address.strip_prefix("http://").expect("loopback URL"))
        .expect("connect WebSocket");
    tcp.set_read_timeout(Some(Duration::from_secs(10)))
        .expect("WebSocket read timeout");
    let (mut socket, _) = tungstenite::client(request, tcp).expect("WebSocket handshake");
    let mut body = request_body(true);
    body["type"] = json!("response.create");
    socket
        .send(Message::Text(body.to_string()))
        .expect("WebSocket request body");

    let mut completed = false;
    for _ in 0..16 {
        let frame = socket
            .read()
            .expect("WebSocket response")
            .into_text()
            .unwrap();
        let event: Value = serde_json::from_str(&frame).expect("Responses event JSON");
        match event["type"].as_str() {
            Some("response.completed") => {
                assert_eq!(event["response"]["id"], "resp_ws_gzip");
                completed = true;
                break;
            }
            Some("response.failed") => panic!("compressed stream failed: {event}"),
            _ => {}
        }
    }
    assert!(completed, "WebSocket must receive response.completed");
    socket.close(None).expect("close WebSocket");
    worker.join().expect("upstream worker");
}
