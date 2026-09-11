use super::*;

#[test]
fn native_responses_expands_only_asb_owned_compaction_payloads() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let file = sandbox_codex_file(
        &state,
        "native owned compact sandbox",
        endpoint(&upstream),
        "vendor-native-key".to_string(),
        CodexUpstream::Responses,
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("activate route");
    let continuation_key = match &projection.activation {
        crate::gateway::GatewayActivation::Routed(route) => route.continuation_key,
        crate::gateway::GatewayActivation::Direct { .. } => panic!("routed projection"),
    };
    let owned = crate::gateway::compaction::seal_for_test(
        "finished parser changes; next run cargo test",
        &continuation_key,
    )
    .expect("seal ASB payload");
    let provider_owned = "provider-opaque-encrypted-content";
    let request_body = json!({
        "model": "sandbox-model",
        "stream": false,
        "input": [
            {"type": "compaction", "encrypted_content": owned},
            {"type": "compaction", "encrypted_content": provider_owned},
        ],
    });
    let worker = thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(10))
            .expect("upstream receive")
            .expect("upstream request");
        assert_eq!(request.url(), "/v1/responses");
        let mut received = String::new();
        request
            .as_reader()
            .read_to_string(&mut received)
            .expect("read upstream body");
        assert!(!received.contains("asb-compaction-v1."));
        let value: Value = serde_json::from_str(&received).expect("upstream JSON");
        assert_eq!(value["input"][0]["type"], "message");
        assert_eq!(value["input"][0]["role"], "user");
        assert!(value["input"][0]["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("next run cargo test")));
        assert_eq!(
            value["input"][1],
            json!({"type": "compaction", "encrypted_content": provider_owned})
        );
        request
            .respond(
                Response::from_string(
                    json!({
                        "id": "resp_native_after_expand",
                        "object": "response",
                        "status": "completed",
                        "model": "sandbox-model",
                        "output": [],
                        "error": null,
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
        .post(codex_endpoint(&projection))
        .header(CONTENT_TYPE, "application/json")
        .body(request_body.to_string())
        .send()
        .expect("native Responses request");
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(
        serde_json::from_str::<Value>(&response.text().expect("response body"))
            .expect("response JSON")["id"],
        "resp_native_after_expand"
    );

    worker.join().expect("upstream worker");
    gateway.shutdown();
}
