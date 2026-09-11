use super::*;
use tiny_http::Header;

#[test]
fn legacy_and_v2_compaction_continue_through_each_protocol_without_exposing_payloads() {
    for protocol in [
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::AnthropicMessages,
    ] {
        run(protocol, false);
    }
}

#[test]
fn bridged_compaction_decodes_compressed_success_response() {
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let file = sandbox_codex_file(
        &state,
        "compressed compact",
        endpoint(&upstream),
        "vendor-key".into(),
        CodexUpstream::ChatCompletions,
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .unwrap();
    gateway.commit(&projection, || Ok(())).unwrap();
    let worker = thread::spawn(move || {
        let request = upstream
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        let response = json!({
            "id": "compressed-summary",
            "object": "chat.completion",
            "model": "sandbox-model",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "compressed summary"},
                "finish_reason": "stop"
            }]
        });
        let compressed =
            zstd::stream::encode_all(std::io::Cursor::new(response.to_string().into_bytes()), 0)
                .unwrap();
        request
            .respond(
                Response::from_data(compressed)
                    .with_header(Header::from_bytes("Content-Encoding", "zstd").unwrap())
                    .with_header(content_type("application/json")),
            )
            .unwrap();
    });
    let response = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
        .post(format!("{}/compact", codex_endpoint(&projection)))
        .body(json!({"model":"sandbox-model","input":[{"type":"message","role":"user","content":"history"}]}).to_string())
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let body: Value = serde_json::from_str(&response.text().unwrap()).unwrap();
    assert_eq!(body["output"][0]["type"], "compaction");
    worker.join().unwrap();
    gateway.shutdown();
}

#[test]
fn native_responses_compact_is_forwarded_without_creating_a_summary() {
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let file = sandbox_codex_file(
        &state,
        "native compact",
        endpoint(&upstream),
        "vendor-key".into(),
        CodexUpstream::Responses,
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .unwrap();
    gateway.commit(&projection, || Ok(())).unwrap();
    let request_body = json!({
        "model": "sandbox-model",
        "input": [{ "type": "message", "role": "user", "content": "keep opaque" }]
    });
    let expected_body = request_body.clone();
    let worker = thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        assert_eq!(request.method(), &Method::Post);
        assert_eq!(request.url(), "/v1/responses/compact");
        let mut received = String::new();
        request.as_reader().read_to_string(&mut received).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&received).unwrap(),
            expected_body
        );
        request
            .respond(
                Response::from_string(
                    json!({
                        "id": "compact_native",
                        "object": "response.compaction",
                        "output": [{
                            "type": "compaction",
                            "encrypted_content": "provider-owned-opaque"
                        }]
                    })
                    .to_string(),
                )
                .with_header(content_type("application/json")),
            )
            .unwrap();
    });
    let response = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
        .post(format!("{}/compact", codex_endpoint(&projection)))
        .body(request_body.to_string())
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let body: Value = serde_json::from_str(&response.text().unwrap()).unwrap();
    assert_eq!(body["id"], "compact_native");
    assert_eq!(
        body["output"][0]["encrypted_content"],
        "provider-owned-opaque"
    );
    worker.join().unwrap();
    gateway.shutdown();
}

#[test]
fn native_responses_v2_compaction_trigger_stays_on_responses() {
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let file = sandbox_codex_file(
        &state,
        "native v2 compact",
        endpoint(&upstream),
        "vendor-key".into(),
        CodexUpstream::Responses,
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .unwrap();
    gateway.commit(&projection, || Ok(())).unwrap();
    let request_body = json!({
        "model": "sandbox-model",
        "input": [
            { "type": "compaction", "encrypted_content": "provider-owned-opaque" },
            { "type": "compaction_trigger" }
        ]
    });
    let expected_body = request_body.clone();
    let worker = thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        assert_eq!(request.method(), &Method::Post);
        assert_eq!(request.url(), "/v1/responses");
        let mut received = String::new();
        request.as_reader().read_to_string(&mut received).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&received).unwrap(),
            expected_body
        );
        request
            .respond(
                Response::from_string(
                    json!({
                        "id": "resp_after_compact",
                        "object": "response",
                        "status": "completed",
                        "model": "sandbox-model",
                        "output": [],
                        "error": null
                    })
                    .to_string(),
                )
                .with_header(content_type("application/json")),
            )
            .unwrap();
    });
    let response = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
        .post(codex_endpoint(&projection))
        .body(request_body.to_string())
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let body: Value = serde_json::from_str(&response.text().unwrap()).unwrap();
    assert_eq!(body["id"], "resp_after_compact");
    worker.join().unwrap();
    gateway.shutdown();
}

fn run(protocol: UpstreamProtocol, minimal: bool) {
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let file = sandbox_codex_file(
        &state,
        "compact",
        endpoint(&upstream),
        "vendor-key".into(),
        codex_upstream(protocol),
    );
    assert!(!minimal, "minimal is not a Codex profile storage mode");
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .unwrap();
    gateway.commit(&projection, || Ok(())).unwrap();
    let worker = thread::spawn(move || summary_upstream(upstream, protocol));
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let initial = json!({"model":"sandbox-model","input":[{"type":"message","role":"user","content":"Fix parser; tests remain."}]});
    let legacy = client
        .post(format!("{}/compact", codex_endpoint(&projection)))
        .bearer_auth("fixture-official-access")
        .body(initial.to_string())
        .send()
        .unwrap();
    assert_eq!(legacy.status().as_u16(), 200);
    let legacy: Value = serde_json::from_str(&legacy.text().unwrap()).unwrap();
    assert_eq!(legacy["output"][0]["type"], "compaction");
    let mut input = legacy["output"].as_array().unwrap().clone();
    input.push(json!({"type":"compaction_trigger"}));
    let v2 = client
        .post(codex_endpoint(&projection))
        .body(json!({"model":"sandbox-model","input":input,"stream":true}).to_string())
        .send()
        .unwrap();
    assert_eq!(v2.status().as_u16(), 200);
    let events = v2.text().unwrap();
    assert_eq!(
        events.matches("event: response.output_item.done\n").count(),
        1
    );
    assert!(events.contains("event: response.completed"));
    assert!(!events.contains("Parser fixed; tests remain."));
    worker.join().unwrap();
    gateway.shutdown();
}

fn summary_upstream(upstream: Server, protocol: UpstreamProtocol) {
    for index in 0..2 {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        assert_eq!(request.method(), &Method::Post);
        assert!(!request.headers().iter().any(
            |header| header.field.equiv("Upgrade") || header.field.equiv("ChatGPT-Account-ID")
        ));
        let mut text = String::new();
        request.as_reader().read_to_string(&mut text).unwrap();
        assert!(!text.contains("asb-compaction-v1."));
        assert!(!text.contains("fixture-official-access"));
        if index == 1 {
            assert!(text.contains("Parser fixed; tests remain."));
        }
        let body: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["stream"], false);
        assert!(body
            .get("tools")
            .is_none_or(|tools| tools.as_array().is_some_and(Vec::is_empty)));
        let response = match protocol {
            UpstreamProtocol::Responses => {
                json!({"id":"summary","object":"response","model":"sandbox-model","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Parser fixed; tests remain."}]}],"error":null})
            }
            UpstreamProtocol::ChatCompletions => {
                json!({"id":"summary","object":"chat.completion","model":"sandbox-model","choices":[{"index":0,"message":{"role":"assistant","content":"Parser fixed; tests remain."},"finish_reason":"stop"}]})
            }
            UpstreamProtocol::AnthropicMessages => {
                json!({"id":"summary","type":"message","role":"assistant","model":"sandbox-model","content":[{"type":"text","text":"Parser fixed; tests remain."}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":100,"output_tokens":20}})
            }
        };
        request
            .respond(
                Response::from_string(response.to_string())
                    .with_header(content_type("application/json")),
            )
            .unwrap();
    }
}
