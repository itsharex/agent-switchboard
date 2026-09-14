use super::*;
use asb_core::contracts::{LocalProxyRequestOverrides, ProviderConnectionOptions};
use std::collections::BTreeMap;

#[test]
fn claude_gateway_applies_connection_overrides_at_the_upstream_exit() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let upstream_base = endpoint(&upstream);
    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let key = "fixture-claude-upstream-key".to_string();
    let mut profile = sandbox_profile(
        &state,
        AppKind::Claude,
        "Claude request override sandbox",
        upstream_base.clone(),
        key.clone(),
        UpstreamProtocol::ChatCompletions,
    );
    profile.base_url = Some(format!("{upstream_base}/custom/messages?fixed=1"));
    profile.connection = connection_options();
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project(&SwitchPlan::direct(
            profile,
            default_client_settings(AppKind::Claude),
        ))
        .expect("project Claude route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("activate route");

    let worker = thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(10))
            .expect("upstream receive")
            .expect("upstream request");
        assert_eq!(request.url(), "/custom/messages?fixed=1&incoming=2");
        assert_header(&request, "user-agent", Some("asb-override-fixture/1"));
        assert_header(&request, "x-provider-tag", Some("override"));
        assert_header(
            &request,
            "authorization",
            Some("Bearer fixture-claude-upstream-key"),
        );
        assert_header(&request, "content-type", Some("application/json"));
        assert_ne!(header(&request, "host"), Some("blocked.example"));
        let mut body = String::new();
        request
            .as_reader()
            .read_to_string(&mut body)
            .expect("read upstream body");
        let body: Value = serde_json::from_str(&body).expect("upstream JSON");
        assert_eq!(body["model"], "override-model");
        assert_eq!(body["metadata"]["tenant"], "fixture");
        assert_eq!(body["stream"], false);
        request
            .respond(
                Response::from_string(
                    json!({
                        "id": "chat_override",
                        "object": "chat.completion",
                        "model": "override-model",
                        "choices": [{
                            "index": 0,
                            "message": {"role": "assistant", "content": "ok"},
                            "finish_reason": "stop"
                        }],
                        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
                    })
                    .to_string(),
                )
                .with_header(content_type("application/json")),
            )
            .expect("respond upstream");
    });

    let response = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("HTTP client")
        .post(format!(
            "{}/v1/messages?incoming=2",
            gateway.configured_base_url()
        ))
        .header(
            "Authorization",
            format!("Bearer {}", projection_token(&projection)),
        )
        .header(CONTENT_TYPE, "application/json")
        .body(
            json!({
                "model": "client-model",
                "max_tokens": 32,
                "stream": false,
                "messages": [{"role": "user", "content": "hello"}]
            })
            .to_string(),
        )
        .send()
        .expect("Claude gateway request");
    assert_eq!(response.status().as_u16(), 200);
    let body: Value = serde_json::from_str(&response.text().expect("Claude response"))
        .expect("Claude response JSON");
    assert_eq!(body["content"][0]["text"], "ok");
    worker.join().expect("upstream worker");
    gateway.shutdown();
}

fn connection_options() -> ProviderConnectionOptions {
    let mut headers = BTreeMap::new();
    headers.insert("x-provider-tag".to_string(), "override".to_string());
    headers.insert("authorization".to_string(), "Bearer blocked".to_string());
    headers.insert("content-type".to_string(), "text/plain".to_string());
    headers.insert("host".to_string(), "blocked.example".to_string());
    headers.insert("content-length".to_string(), "1".to_string());
    ProviderConnectionOptions {
        is_full_url: true,
        custom_user_agent: Some("asb-override-fixture/1".to_string()),
        local_proxy_request_overrides: Some(LocalProxyRequestOverrides {
            headers,
            body: json!({
                "model": "override-model",
                "metadata": {"tenant": "fixture"},
                "stream": true
            }),
        }),
        ..Default::default()
    }
}

fn header<'a>(request: &'a tiny_http::Request, name: &str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|header| header.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str())
}

fn assert_header(request: &tiny_http::Request, name: &str, expected: Option<&str>) {
    assert_eq!(header(request, name), expected, "unexpected {name} header");
}
