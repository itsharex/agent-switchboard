use super::*;
use tiny_http::{Header, StatusCode};

#[test]
fn codex_models_are_forwarded_to_the_selected_openai_api_root() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let file = sandbox_codex_file(
        &state,
        "models operation",
        endpoint(&upstream),
        "vendor-models-key".into(),
        CodexUpstream::Responses,
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("commit route");

    let models_url = codex_endpoint(&projection).replace("/responses", "/models?limit=1");
    let response = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("HTTP client")
        .get(models_url)
        .header("Authorization", "Bearer official-access-token")
        .send()
        .expect("gateway models response");
    assert_eq!(response.status().as_u16(), 200);
    let body: Value =
        serde_json::from_str(&response.text().expect("models body")).expect("models JSON");
    assert!(body.get("data").is_none());
    assert!(body.get("object").is_none());
    assert_eq!(body["models"][0]["slug"], "sandbox-model");
    assert_eq!(body["models"][0]["context_window"], 128_000);
    assert_eq!(body["models"][0]["max_output_tokens"], 16_384);
    assert_eq!(
        body["models"][0]["input_modalities"],
        json!(["text", "image"])
    );
    assert!(body["models"][0]["supported_reasoning_levels"].is_array());
    let expected: Value =
        serde_json::from_str(&file.model_catalog_json().expect("catalog projection"))
            .expect("catalog JSON");
    assert_eq!(body, expected);
    assert!(upstream
        .recv_timeout(Duration::from_millis(100))
        .expect("models projection does not call upstream")
        .is_none());
    gateway.shutdown();
}

#[test]
fn codex_models_use_the_local_catalog_when_upstream_models_are_unavailable() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let mut file = sandbox_codex_file(
        &state,
        "local models catalog",
        endpoint(&upstream),
        "vendor-models-key".into(),
        CodexUpstream::Responses,
    );
    file.profile.capabilities.models = false;
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("commit route");

    let models_url = codex_endpoint(&projection).replace("/responses", "/models");
    let response = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("HTTP client")
        .get(models_url)
        .send()
        .expect("gateway models response");
    assert_eq!(response.status().as_u16(), 200);
    let body: Value =
        serde_json::from_str(&response.text().expect("models body")).expect("models JSON");
    assert_eq!(body["models"][0]["slug"], "sandbox-model");
    assert!(upstream
        .recv_timeout(Duration::from_millis(100))
        .expect("models projection does not call upstream")
        .is_none());
    gateway.shutdown();
}

#[test]
fn codex_image_operation_rejects_an_anthropic_upstream_explicitly() {
    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let file = sandbox_codex_file(
        &state,
        "anthropic operation",
        "http://127.0.0.1:9".into(),
        "vendor-anthropic-key".into(),
        CodexUpstream::AnthropicMessages,
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("commit route");

    let image_url = codex_endpoint(&projection).replace("/responses", "/images/generations");
    let response = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("HTTP client")
        .post(image_url)
        .header(CONTENT_TYPE, "application/json")
        .body(json!({"model": "sandbox-model", "prompt": "fixture"}).to_string())
        .send()
        .expect("gateway rejection");
    assert_eq!(response.status().as_u16(), 501);
    let body = response.text().expect("error body");
    assert!(body.contains("不支持此 Codex 操作"));
    assert!(!body.contains("vendor-anthropic-key"));
    gateway.shutdown();
}

#[test]
fn codex_chat_operation_relays_sse_without_responses_conversion() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let file = sandbox_codex_file(
        &state,
        "chat operation",
        endpoint(&upstream),
        "vendor-chat-key".into(),
        CodexUpstream::ChatCompletions,
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("commit route");

    let worker = thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(10))
            .expect("receive upstream request")
            .expect("upstream request");
        assert_eq!(request.method(), &Method::Post);
        assert_eq!(request.url(), "/v1/chat/completions");
        let mut body = String::new();
        request
            .as_reader()
            .read_to_string(&mut body)
            .expect("read body");
        assert_eq!(
            serde_json::from_str::<Value>(&body).expect("chat body")["stream"],
            true
        );
        request
            .respond(
                Response::from_string(
                    "data: {\"id\":\"chat_fixture\",\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\ndata: [DONE]\n\n",
                )
                .with_header(content_type("text/event-stream")),
            )
            .expect("respond upstream");
    });

    let chat_url = codex_endpoint(&projection).replace("/responses", "/chat/completions");
    let response = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("HTTP client")
        .post(chat_url)
        .header(CONTENT_TYPE, "application/json")
        .body(json!({"model": "sandbox-model", "messages": [], "stream": true}).to_string())
        .send()
        .expect("gateway stream");
    assert!(response
        .headers()
        .get(CONTENT_TYPE)
        .expect("stream content type")
        .to_str()
        .expect("valid content type")
        .starts_with("text/event-stream"));
    let body = response.text().expect("stream body");
    assert!(body.contains("chat_fixture"));
    assert!(body.contains("[DONE]"));

    worker.join().expect("upstream worker");
    gateway.shutdown();
}

#[test]
fn codex_json_operations_use_the_declared_path_and_preserve_upstream_facts() {
    for (operation, body) in [
        ("alpha/search", json!({"query": "fixture"})),
        (
            "images/generations",
            json!({"model": "sandbox-model", "prompt": "fixture"}),
        ),
        (
            "images/edits",
            json!({"model": "sandbox-model", "prompt": "fixture", "image": "fixture"}),
        ),
    ] {
        relay_json_operation(operation, body);
    }
}

#[test]
fn codex_catalog_capabilities_reject_unsupported_requests_before_network() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let mut file = sandbox_codex_file(
        &state,
        "restricted model",
        endpoint(&upstream),
        "vendor-restricted-key".into(),
        CodexUpstream::Responses,
    );
    let model = &mut file.profile.catalog[0];
    model.images = false;
    model.compact = false;
    model.function_tools = false;
    model.tool_search = false;
    file.profile.capabilities.compact = true;
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("commit route");
    let responses = codex_endpoint(&projection);
    let compact = format!("{responses}/compact");
    let image = responses.replace("/responses", "/images/generations");
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("HTTP client");
    for (url, body, reason) in [
        (
            responses.clone(),
            json!({"model":"sandbox-model","input":[{"type":"compaction_trigger"}]}),
            "原生压缩能力",
        ),
        (
            compact,
            json!({"model":"sandbox-model","input":[]}),
            "原生压缩能力",
        ),
        (
            responses,
            json!({"model":"sandbox-model","input":[],"tools":[{"type":"function","name":"fixture","parameters":{}}]}),
            "工具能力",
        ),
        (
            codex_endpoint(&projection),
            json!({"model":"sandbox-model","input":[],"tools":[{"type":"namespace","name":"functions.","tools":[{"type":"function","name":"fixture","parameters":{}}]}]}),
            "工具能力",
        ),
        (
            codex_endpoint(&projection),
            json!({"model":"sandbox-model","input":[],"tools":[{"type":"tool_search"}]}),
            "工具能力",
        ),
        (
            image,
            json!({"model":"sandbox-model","prompt":"fixture"}),
            "图像能力",
        ),
    ] {
        let response = client
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .expect("gateway capability rejection");
        assert_eq!(response.status().as_u16(), 422);
        assert!(response.text().expect("error body").contains(reason));
    }
    assert!(upstream
        .recv_timeout(Duration::from_millis(100))
        .expect("no upstream request")
        .is_none());
    gateway.shutdown();
}

fn relay_json_operation(operation: &str, body: Value) {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let mut file = sandbox_codex_file(
        &state,
        "typed JSON operation",
        endpoint(&upstream),
        "vendor-operation-key".into(),
        CodexUpstream::Responses,
    );
    file.profile.model_routes[0].upstream_model = "vendor-operation-model".to_string();
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("commit route");
    let expected_path = format!("/v1/{operation}");
    let mut expected_body = body.clone();
    if let Some(model) = expected_body.get_mut("model") {
        *model = Value::String("vendor-operation-model".to_string());
    }
    let worker = thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(10))
            .expect("receive upstream request")
            .expect("upstream request");
        assert_eq!(request.method(), &Method::Post);
        assert_eq!(request.url(), expected_path);
        let mut received = String::new();
        request
            .as_reader()
            .read_to_string(&mut received)
            .expect("read operation body");
        assert_eq!(
            serde_json::from_str::<Value>(&received).unwrap(),
            expected_body
        );
        request
            .respond(
                Response::from_string(json!({"object":"operation.fixture"}).to_string())
                    .with_status_code(StatusCode(202))
                    .with_header(content_type("application/json"))
                    .with_header(Header::from_bytes(b"x-request-id", b"req-operation-42").unwrap())
                    .with_header(Header::from_bytes(b"set-cookie", b"vendor=session").unwrap()),
            )
            .expect("respond upstream");
    });
    let url = codex_endpoint(&projection).replace("/responses", &format!("/{operation}"));
    let response = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("HTTP client")
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .send()
        .expect("gateway operation response");
    assert_eq!(response.status().as_u16(), 202);
    assert_eq!(response.headers()["x-request-id"], "req-operation-42");
    assert!(response.headers().get("set-cookie").is_none());
    assert_eq!(
        serde_json::from_str::<Value>(&response.text().expect("operation body")).unwrap()["object"],
        "operation.fixture"
    );
    worker.join().expect("upstream worker");
    gateway.shutdown();
}
