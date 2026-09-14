use super::*;
use crate::claude_auth::{ClaudeAccount, ClaudeAuth, TestEndpoints};
use asb_core::claude_auth::ClaudeAuthProvider;

fn account(id: &str) -> ClaudeAccount {
    ClaudeAccount {
        id: id.into(),
        label: id.into(),
        provider: ClaudeAuthProvider::CodexOauth,
        access_token: format!("managed-secret-{id}"),
        refresh_token: Some("managed-refresh".into()),
        expires_at_ms: Some(chrono::Utc::now().timestamp_millis() + 3_600_000),
        upstream_account_id: Some(format!("workspace-{id}")),
        github_domain: None,
    }
}

fn fixture(local: &LocalState, base: &str) -> (GatewayController, String, Arc<ClaudeAuth>) {
    let auth = ClaudeAuth::register_test_endpoints(
        local.root(),
        TestEndpoints {
            github_api: base.into(),
            oauth_token: base.into(),
            upstream: format!("{base}/v1"),
        },
    );
    auth.save_account(account("alice"), &auth.view().unwrap().file_hash, true)
        .unwrap();
    let mut profile = sandbox_profile(
        local,
        AppKind::Claude,
        "Claude managed",
        format!("{base}/v1"),
        "unused-seed".into(),
        UpstreamProtocol::Responses,
    );
    // A source-created managed draft is keyless before it reaches storage.
    profile.connection.provider_type = Some("codex_oauth".into());
    profile.api_key.clear();
    let mut value = serde_json::to_value(&profile).unwrap();
    value.as_object_mut().unwrap().remove("id");
    let draft: ProviderDraft = serde_json::from_value(value).unwrap();
    let record = local
        .configuration()
        .find_provider_record(&profile.id)
        .unwrap();
    let profile = local
        .configuration()
        .update_provider(&profile.id, draft, &record.file_hash)
        .unwrap()
        .profile;
    let gateway = GatewayController::start(local);
    let projection = gateway
        .project_with_candidates(
            local,
            &SwitchPlan::direct(profile, default_client_settings(AppKind::Claude)),
        )
        .unwrap();
    let token = projection_token(&projection);
    gateway.commit(&projection, || Ok(())).unwrap();
    (gateway, token, auth)
}

fn send(gateway: &GatewayController, token: &str, stream: bool) -> reqwest::blocking::Response {
    Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(8))
        .build()
        .unwrap()
        .post(format!("{}/v1/messages", gateway.configured_base_url()))
        .bearer_auth(token)
        .header(CONTENT_TYPE, "application/json")
        .body(
            json!({"model":"claude-sonnet", "max_tokens":32, "stream":stream,
            "messages":[{"role":"user","content":"hello"}]})
            .to_string(),
        )
        .send()
        .unwrap()
}

fn completed() -> String {
    format!(
        "event: response.completed\ndata: {}\n\ndata: [DONE]\n\n",
        json!({"type":"response.completed","response":{
            "id":"resp-fixture", "object":"response", "status":"completed", "model":"gpt-fixture",
            "output":[{"type":"message", "id":"msg-fixture", "role":"assistant", "status":"completed",
                "content":[{"type":"output_text", "text":"managed reply", "annotations":[]}]}],
            "usage":{"input_tokens":10,"output_tokens":3}
        }})
    )
}

#[test]
fn claude_managed_gateway_uses_the_selected_account_and_buffers_for_non_streaming_clients() {
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let server = Server::http("127.0.0.1:0").unwrap();
    let base = endpoint(&server);
    let task = thread::spawn(move || {
        for id in ["alice", "bob"] {
            let mut request = server
                .recv_timeout(Duration::from_secs(8))
                .unwrap()
                .unwrap();
            assert_eq!(request.url(), "/v1/responses");
            let header = |name| {
                request
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv(name))
                    .unwrap()
                    .value
                    .to_string()
            };
            assert_eq!(
                header("authorization"),
                format!("Bearer managed-secret-{id}")
            );
            assert_eq!(header("chatgpt-account-id"), format!("workspace-{id}"));
            assert_eq!(header("originator"), "codex_cli_rs");
            let mut body = String::new();
            request.as_reader().read_to_string(&mut body).unwrap();
            let body: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(body["stream"], true);
            assert_eq!(body["store"], false);
            assert!(body.get("max_output_tokens").is_none());
            request
                .respond(
                    Response::from_string(completed())
                        .with_header(content_type("text/event-stream")),
                )
                .unwrap();
        }
    });
    let (gateway, token, auth) = fixture(&local, &base);
    for id in ["alice", "bob"] {
        if id == "bob" {
            auth.save_account(account(id), &auth.view().unwrap().file_hash, true)
                .unwrap();
        }
        let response = send(&gateway, &token, false);
        assert_eq!(response.status().as_u16(), 200);
        let body: Value = serde_json::from_str(&response.text().unwrap()).unwrap();
        assert_eq!(body["content"][0]["text"], "managed reply");
        assert_eq!(body["usage"]["input_tokens"], 10);
    }
    task.join().unwrap();
    gateway.shutdown();
    assert!(!directory.path().join(".codex/auth.json").exists());
}

#[test]
fn missing_managed_account_does_not_send_a_placeholder_or_fall_back_to_native_login() {
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let server = Server::http("127.0.0.1:0").unwrap();
    let (gateway, token, auth) = fixture(&local, &endpoint(&server));
    auth.remove("alice", &auth.view().unwrap().file_hash)
        .unwrap();
    let response = send(&gateway, &token, false);
    assert_eq!(response.status().as_u16(), 401);
    let body = response.text().unwrap();
    assert!(!body.contains("managed-secret"));
    assert!(server
        .recv_timeout(Duration::from_millis(100))
        .unwrap()
        .is_none());
    gateway.shutdown();
}
