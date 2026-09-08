use super::*;

fn client_request(app: AppKind) -> Value {
    match app {
        AppKind::Codex => json!({
            "model": "sandbox-model",
            "input": "hello through the gateway",
            "max_output_tokens": 32,
        }),
        AppKind::Claude => json!({
            "model": "sandbox-model",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello through the gateway" }],
        }),
    }
}

fn upstream_response(protocol: UpstreamProtocol) -> Value {
    match protocol {
        UpstreamProtocol::Responses => json!({
            "id": "resp_sandbox",
            "object": "response",
            "status": "completed",
            "model": "sandbox-model",
            "output": [{
                "type": "message",
                "id": "msg_sandbox",
                "status": "completed",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "converted upstream answer", "annotations": [] }],
            }],
            "usage": { "input_tokens": 3, "output_tokens": 4, "total_tokens": 7 },
            "error": null,
        }),
        UpstreamProtocol::ChatCompletions => json!({
            "id": "chat_sandbox",
            "object": "chat.completion",
            "created": 0,
            "model": "sandbox-model",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "converted upstream answer" },
                "finish_reason": "stop",
            }],
            "usage": { "prompt_tokens": 3, "completion_tokens": 4, "total_tokens": 7 },
        }),
        UpstreamProtocol::AnthropicMessages => json!({
            "id": "msg_sandbox",
            "type": "message",
            "role": "assistant",
            "model": "sandbox-model",
            "content": [{"type": "text", "text": "converted upstream answer"}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 3, "output_tokens": 4},
        }),
    }
}

fn assert_upstream_request(protocol: UpstreamProtocol, body: &Value) {
    match protocol {
        UpstreamProtocol::Responses => {
            assert_eq!(body["input"][0]["role"], "user");
            assert_eq!(
                body["input"][0]["content"][0]["text"],
                "hello through the gateway"
            );
        }
        UpstreamProtocol::ChatCompletions => {
            assert_eq!(body["messages"][0]["role"], "user");
            assert_eq!(body["messages"][0]["content"], "hello through the gateway");
        }
        UpstreamProtocol::AnthropicMessages => {
            assert_eq!(body["messages"][0]["role"], "user");
            assert_eq!(
                body["messages"][0]["content"][0]["text"],
                "hello through the gateway"
            );
        }
    }
}

fn run_cross_protocol_case(app: AppKind, upstream_protocol: UpstreamProtocol) {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let upstream_url = endpoint(&upstream);
    let upstream_key = format!("fixture-{}", uuid::Uuid::new_v4());
    let (observed_sender, observed_receiver) = mpsc::channel();
    let upstream_worker = serve_cross_protocol(upstream, upstream_protocol, observed_sender);

    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = sandbox_profile(
        &state,
        app,
        "cross protocol sandbox",
        upstream_url,
        upstream_key.clone(),
        upstream_protocol,
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project(&SwitchPlan::direct(profile, default_client_settings(app)))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("commit route");
    let client = Client::builder().no_proxy().build().expect("client");
    let gateway_url = match app {
        AppKind::Codex => codex_endpoint(&projection),
        AppKind::Claude => format!("{}/v1/messages", gateway.configured_base_url()),
    };
    let response = client
        .post(gateway_url)
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                if app == AppKind::Codex {
                    "fixture-official-access".to_string()
                } else {
                    projection_token(&projection)
                }
            ),
        )
        .header(CONTENT_TYPE, "application/json")
        .body(serde_json::to_vec(&client_request(app)).expect("client request json"))
        .send()
        .expect("gateway request");
    assert_client_response(app, response);

    let (path, authorization, x_api_key, anthropic_version, body) = observed_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("one upstream request");
    let expected_path = match upstream_protocol {
        UpstreamProtocol::Responses => "/v1/responses",
        UpstreamProtocol::ChatCompletions => "/v1/chat/completions",
        UpstreamProtocol::AnthropicMessages => "/v1/messages",
    };
    assert_eq!(path, expected_path);
    match upstream_protocol.authentication_scheme() {
        AuthenticationScheme::Bearer => {
            assert_eq!(authorization, Some(format!("Bearer {upstream_key}")));
            assert!(x_api_key.is_none());
            assert!(anthropic_version.is_none());
        }
        AuthenticationScheme::XApiKey => {
            assert!(authorization.is_none());
            assert_eq!(x_api_key.as_deref(), Some(upstream_key.as_str()));
            assert_eq!(anthropic_version.as_deref(), Some("2023-06-01"));
        }
    }
    assert_upstream_request(
        upstream_protocol,
        &serde_json::from_str(&body).expect("upstream request json"),
    );
    upstream_worker.join().expect("upstream worker");
    gateway.shutdown();
}

#[test]
fn loopback_gateway_converts_every_cross_protocol_pair() {
    for (app, upstream_protocol) in [
        (AppKind::Codex, UpstreamProtocol::ChatCompletions),
        (AppKind::Codex, UpstreamProtocol::AnthropicMessages),
        (AppKind::Claude, UpstreamProtocol::Responses),
        (AppKind::Claude, UpstreamProtocol::ChatCompletions),
    ] {
        run_cross_protocol_case(app, upstream_protocol);
    }
}

#[test]
fn loopback_gateway_converts_and_isolates_anthropic_upstream_credentials() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let upstream_url = endpoint(&upstream);
    let (observed_sender, observed_receiver) = mpsc::channel();
    let upstream_worker = serve_anthropic(upstream, observed_sender);

    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = sandbox_profile(
        &state,
        AppKind::Codex,
        "Anthropic sandbox",
        upstream_url,
        "upstream-x-api-key".to_string(),
        UpstreamProtocol::AnthropicMessages,
    );
    let gateway = GatewayController::start(&state);
    let plan = SwitchPlan::direct(profile, default_client_settings(AppKind::Codex));
    let projection = gateway.project(&plan).expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("commit route");
    let client = Client::builder().no_proxy().build().expect("client");
    let gateway_url = codex_endpoint(&projection);
    let request_body = serde_json::to_vec(&json!({
        "model": "sandbox-model",
        "input": "hello",
    }))
    .expect("request json");

    let unauthorized = client
        .post(gateway_url.replace(&projection_token(&projection), &revoked_token(&projection)))
        .header("Authorization", "Bearer fake-official-oauth")
        .header(CONTENT_TYPE, "application/json")
        .body(request_body.clone())
        .send()
        .expect("unauthorized request");
    assert_eq!(unauthorized.status(), reqwest::StatusCode::FORBIDDEN);
    let unauthorized_text = unauthorized.text().expect("unauthorized body");
    assert!(!unauthorized_text.contains("upstream-x-api-key"));
    assert!(!unauthorized_text.contains("127.0.0.1"));

    let response = client
        .post(&gateway_url)
        .header("Authorization", "Bearer fixture-official-access")
        .header(CONTENT_TYPE, "application/json")
        .body(request_body)
        .send()
        .expect("gateway request");
    assert!(response.status().is_success());
    let body: Value = serde_json::from_str(&response.text().expect("converted response body"))
        .expect("converted response json");
    assert_eq!(body["object"], "response");
    assert_eq!(
        body["output"][0]["content"][0]["text"],
        "converted upstream answer"
    );

    let (url, x_api_key, authorization, anthropic_version, body) = observed_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("one upstream request");
    assert_eq!(url, "/v1/messages");
    assert_eq!(x_api_key.as_deref(), Some("upstream-x-api-key"));
    assert!(authorization.is_none());
    assert_eq!(anthropic_version.as_deref(), Some("2023-06-01"));
    let upstream_body: Value = serde_json::from_str(&body).expect("anthropic request json");
    assert_eq!(upstream_body["messages"][0]["role"], "user");
    assert_eq!(upstream_body["messages"][0]["content"][0]["text"], "hello");
    assert_eq!(upstream_body["max_tokens"], 8_192);

    upstream_worker.join().expect("upstream worker");
    gateway.shutdown();
}

fn serve_cross_protocol(
    upstream: Server,
    upstream_protocol: UpstreamProtocol,
    observed_sender: mpsc::Sender<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    )>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(10))
            .expect("upstream receive")
            .expect("upstream request");
        let path = request.url().to_string();
        let authorization = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("Authorization"))
            .map(|header| header.value.as_str().to_string());
        let x_api_key = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("x-api-key"))
            .map(|header| header.value.as_str().to_string());
        let anthropic_version = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("anthropic-version"))
            .map(|header| header.value.as_str().to_string());
        let mut body = String::new();
        request
            .as_reader()
            .read_to_string(&mut body)
            .expect("read upstream body");
        observed_sender
            .send((path, authorization, x_api_key, anthropic_version, body))
            .expect("report upstream request");
        let response = Response::from_data(
            serde_json::to_vec(&upstream_response(upstream_protocol)).expect("response json"),
        )
        .with_header(content_type("application/json"));
        request.respond(response).expect("respond upstream");
    })
}

fn serve_anthropic(
    upstream: Server,
    observed_sender: mpsc::Sender<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    )>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(10))
            .expect("upstream receive")
            .expect("upstream request");
        let url = request.url().to_string();
        let x_api_key = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("x-api-key"))
            .map(|header| header.value.as_str().to_string());
        let authorization = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("Authorization"))
            .map(|header| header.value.as_str().to_string());
        let anthropic_version = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("anthropic-version"))
            .map(|header| header.value.as_str().to_string());
        let mut body = String::new();
        request
            .as_reader()
            .read_to_string(&mut body)
            .expect("read upstream body");
        observed_sender
            .send((url, x_api_key, authorization, anthropic_version, body))
            .expect("report upstream request");
        let response = Response::from_data(
            serde_json::to_vec(&json!({
                "id": "msg_sandbox",
                "type": "message",
                "role": "assistant",
                "model": "sandbox-model",
                "content": [{"type": "text", "text": "converted upstream answer"}],
                "stop_reason": "end_turn",
                "stop_sequence": null,
                "usage": {"input_tokens": 3, "output_tokens": 4}
            }))
            .expect("response json"),
        )
        .with_header(content_type("application/json"));
        request.respond(response).expect("respond upstream");
    })
}

fn assert_client_response(app: AppKind, response: reqwest::blocking::Response) {
    assert!(response.status().is_success());
    let client_body: Value = serde_json::from_str(&response.text().expect("client response"))
        .expect("client response json");
    match app {
        AppKind::Codex => {
            assert_eq!(client_body["object"], "response");
            assert_eq!(
                client_body["output"][0]["content"][0]["text"],
                "converted upstream answer"
            );
        }
        AppKind::Claude => {
            assert_eq!(client_body["type"], "message");
            assert_eq!(
                client_body["content"][0]["text"],
                "converted upstream answer"
            );
        }
    }
}

fn revoked_token(projection: &crate::gateway::GatewayProjection) -> String {
    let mut token = projection_token(projection);
    let last = token.pop().unwrap();
    token.push(if last == '0' { '1' } else { '0' });
    token
}
