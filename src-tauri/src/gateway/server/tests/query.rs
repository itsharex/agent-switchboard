use super::websocket_roundtrip::{
    receive_responses_completion, send_websocket_text, websocket_connect_with_query,
    websocket_request,
};
use super::*;

#[test]
fn responses_and_native_compact_preserve_request_queries() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let file = sandbox_codex_file(
        &state,
        "native query sandbox",
        endpoint(&upstream),
        "vendor-query-key".to_string(),
        CodexUpstream::Responses,
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("activate route");

    let worker = thread::spawn(move || {
        for (path, response) in [
            (
                "/v1/responses?trace=request%20id&mode=standard",
                json!({
                    "id": "resp_query",
                    "object": "response",
                    "status": "completed",
                    "model": "sandbox-model",
                    "output": [],
                    "error": null,
                }),
            ),
            (
                "/v1/responses/compact?trace=compact&mode=native",
                json!({
                    "id": "compact_query",
                    "object": "response.compaction",
                    "output": [{"type": "compaction", "encrypted_content": "provider-owned"}],
                }),
            ),
        ] {
            let request = upstream
                .recv_timeout(Duration::from_secs(10))
                .expect("upstream receive")
                .expect("upstream request");
            assert_eq!(request.url(), path);
            request
                .respond(
                    Response::from_string(response.to_string())
                        .with_header(content_type("application/json")),
                )
                .expect("respond upstream");
        }
    });

    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("HTTP client");
    let responses = client
        .post(format!(
            "{}?trace=request%20id&mode=standard",
            codex_endpoint(&projection)
        ))
        .header(CONTENT_TYPE, "application/json")
        .body(json!({"model": "sandbox-model", "input": "hello"}).to_string())
        .send()
        .expect("Responses request");
    assert_eq!(responses.status().as_u16(), 200);
    let compact = client
        .post(format!(
            "{}/compact?trace=compact&mode=native",
            codex_endpoint(&projection)
        ))
        .header(CONTENT_TYPE, "application/json")
        .body(
            json!({
                "model": "sandbox-model",
                "input": [{"type": "message", "role": "user", "content": "history"}],
            })
            .to_string(),
        )
        .send()
        .expect("native compact request");
    assert_eq!(compact.status().as_u16(), 200);

    worker.join().expect("upstream worker");
    gateway.shutdown();
}

#[test]
fn bridged_compact_preserves_request_query() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let file = sandbox_codex_file(
        &state,
        "bridge query sandbox",
        endpoint(&upstream),
        "vendor-query-key".to_string(),
        CodexUpstream::ChatCompletions,
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("activate route");

    let worker = thread::spawn(move || {
        let request = upstream
            .recv_timeout(Duration::from_secs(10))
            .expect("upstream receive")
            .expect("upstream request");
        assert_eq!(
            request.url(),
            "/v1/chat/completions?trace=bridge&source=test"
        );
        request
            .respond(
                Response::from_string(
                    json!({
                        "id": "chat_query",
                        "object": "chat.completion",
                        "model": "sandbox-model",
                        "choices": [{
                            "index": 0,
                            "message": {"role": "assistant", "content": "summary"},
                            "finish_reason": "stop",
                        }],
                    })
                    .to_string(),
                )
                .with_header(content_type("application/json")),
            )
            .expect("respond upstream");
    });

    let response = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("HTTP client")
        .post(format!(
            "{}/compact?trace=bridge&source=test",
            codex_endpoint(&projection)
        ))
        .header(CONTENT_TYPE, "application/json")
        .body(
            json!({
                "model": "sandbox-model",
                "input": [{"type": "message", "role": "user", "content": "history"}],
            })
            .to_string(),
        )
        .send()
        .expect("bridged compact request");
    assert_eq!(response.status().as_u16(), 200);
    let body: Value =
        serde_json::from_str(&response.text().expect("compact body")).expect("compact JSON");
    assert_eq!(body["output"][0]["type"], "compaction");

    worker.join().expect("upstream worker");
    gateway.shutdown();
}

#[test]
fn websocket_handshake_query_reaches_the_selected_upstream() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let file = sandbox_codex_file(
        &state,
        "WebSocket query sandbox",
        endpoint(&upstream),
        "vendor-query-key".to_string(),
        CodexUpstream::ChatCompletions,
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("activate route");

    let worker = thread::spawn(move || {
        let request = upstream
            .recv_timeout(Duration::from_secs(10))
            .expect("upstream receive")
            .expect("upstream request");
        assert_eq!(
            request.url(),
            "/v1/chat/completions?trace=websocket&phase=1"
        );
        let stream = concat!(
            "data: {\"id\":\"chat_query_ws\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chat_query_ws\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"query response\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chat_query_ws\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        request
            .respond(
                Response::from_string(stream)
                    .with_header(content_type("text/event-stream; charset=utf-8")),
            )
            .expect("respond upstream");
    });

    let address = gateway
        .configured_base_url()
        .strip_prefix("http://")
        .expect("loopback address")
        .to_string();
    let mut socket = websocket_connect_with_query(
        &address,
        &projection_token(&projection),
        "trace=websocket&phase=1",
    );
    send_websocket_text(
        &mut socket,
        &websocket_request(
            json!([{
                "type": "message",
                "id": "query-message",
                "role": "user",
                "content": [{"type": "input_text", "text": "hello"}],
            }]),
            None,
        ),
    );
    let completed = receive_responses_completion(&mut socket);
    assert_eq!(completed["response"]["id"], "chat_query_ws");
    drop(socket);

    worker.join().expect("upstream worker");
    gateway.shutdown();
}
