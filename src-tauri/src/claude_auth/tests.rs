use super::*;
use asb_core::contracts::ProviderAuthBinding;
use serde_json::json;

fn account(id: &str, provider: ClaudeAuthProvider) -> ClaudeAccount {
    ClaudeAccount {
        id: id.into(),
        label: format!("测试 {id}"),
        provider,
        access_token: format!("fake-{id}-access"),
        refresh_token: Some("fake-refresh".into()),
        expires_at_ms: Some(chrono::Utc::now().timestamp_millis() + 3_600_000),
        upstream_account_id: Some(format!("workspace-{id}")),
        github_domain: None,
    }
}

fn options(provider: &str, id: Option<&str>) -> ProviderConnectionOptions {
    ProviderConnectionOptions {
        provider_type: Some(provider.into()),
        auth_binding: Some(ProviderAuthBinding {
            source: "managed_account".into(),
            auth_provider: Some(provider.into()),
            account_id: id.map(str::to_string),
        }),
        ..Default::default()
    }
}

#[test]
fn account_crud_is_revision_guarded_and_views_never_contain_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let auth = ClaudeAuth::new(dir.path());
    let empty = auth.view().unwrap();
    let saved = auth
        .save_account(
            account("alice", ClaudeAuthProvider::CodexOauth),
            &empty.file_hash,
            true,
        )
        .unwrap();
    let serialized = serde_json::to_string(&saved).unwrap();
    assert!(!serialized.contains("fake-alice-access"));
    assert!(!serialized.contains("fake-refresh"));
    assert!(auth
        .save_account(
            account("bob", ClaudeAuthProvider::CodexOauth),
            &empty.file_hash,
            true
        )
        .is_err());
    assert_eq!(
        auth.resolve(&options("codex_oauth", None))
            .unwrap()
            .unwrap()
            .account_id,
        "alice"
    );
    assert!(auth
        .resolve(&options("codex_oauth", Some("missing")))
        .is_err());
    let saved = auth
        .save_account(
            account("bob", ClaudeAuthProvider::CodexOauth),
            &saved.file_hash,
            false,
        )
        .unwrap();
    let saved = auth
        .set_default(ClaudeAuthProvider::CodexOauth, "bob", &saved.file_hash)
        .unwrap();
    assert_eq!(
        auth.resolve(&options("codex_oauth", None))
            .unwrap()
            .unwrap()
            .account_id,
        "bob"
    );
    assert_eq!(
        auth.resolve(&options("codex_oauth", Some("alice")))
            .unwrap()
            .unwrap()
            .account_id,
        "alice"
    );
    auth.remove("alice", &saved.file_hash).unwrap();
    assert!(auth
        .resolve(&options("codex_oauth", Some("alice")))
        .is_err());
    assert!(!dir.path().join("auth.json").exists());
    assert!(!dir.path().join(".credentials.json").exists());
}

#[test]
fn damaged_account_files_and_invalid_defaults_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let auth = ClaudeAuth::new(dir.path());
    let empty = auth.view().unwrap();
    assert!(auth
        .set_default(ClaudeAuthProvider::CodexOauth, "missing", &empty.file_hash)
        .is_err());
    std::fs::write(store::path(dir.path()), "{broken-secret-content").unwrap();
    assert!(auth.view().is_err());
    assert!(auth
        .save_account(
            account("alice", ClaudeAuthProvider::CodexOauth),
            &empty.file_hash,
            true
        )
        .is_err());
    assert_eq!(
        std::fs::read_to_string(store::path(dir.path())).unwrap(),
        "{broken-secret-content"
    );
}

#[test]
fn expired_accounts_refresh_once_and_preserve_the_rotated_token() {
    let dir = tempfile::tempdir().unwrap();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr());
    let task = std::thread::spawn(move || {
        let mut request = server
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        let mut body = String::new();
        request.as_reader().read_to_string(&mut body).unwrap();
        assert!(body.contains("grant_type=refresh_token"));
        assert!(body.contains("refresh_token=fake-refresh"));
        request.respond(tiny_http::Response::from_string(json!({"access_token":"fresh-access", "refresh_token":"rotated-refresh", "expires_in":3600}).to_string())).unwrap();
    });
    let auth = ClaudeAuth::with_endpoints(
        dir.path(),
        TestEndpoints {
            github_api: base.clone(),
            oauth_token: base.clone(),
            upstream: base,
        },
    );
    let mut expired = account("alice", ClaudeAuthProvider::CodexOauth);
    expired.expires_at_ms = Some(1);
    auth.save_account(expired, &auth.view().unwrap().file_hash, true)
        .unwrap();
    assert_eq!(
        auth.resolve(&options("codex_oauth", None))
            .unwrap()
            .unwrap()
            .access_token,
        "fresh-access"
    );
    assert_eq!(
        auth.resolve(&options("codex_oauth", None))
            .unwrap()
            .unwrap()
            .access_token,
        "fresh-access"
    );
    assert!(std::fs::read_to_string(store::path(dir.path()))
        .unwrap()
        .contains("rotated-refresh"));
    task.join().unwrap();
}

#[test]
fn managed_headers_and_body_follow_the_selected_account_not_renderer_overrides() {
    let resolved = ResolvedAccount {
        provider: ClaudeAuthProvider::CodexOauth,
        account_id: "alice".into(),
        access_token: "fresh-secret".into(),
        expires_at_ms: 100,
        endpoint: "https://chatgpt.com/backend-api/codex".into(),
        upstream_account_id: Some("workspace".into()),
    };
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("x-api-key", "old-key".parse().unwrap());
    headers.insert("chatgpt-account-id", "wrong-account".parse().unwrap());
    request::headers(&resolved, &mut headers);
    assert_eq!(headers["authorization"], "Bearer fresh-secret");
    assert_eq!(headers["chatgpt-account-id"], "workspace");
    assert!(!headers.contains_key("x-api-key"));
    let mut body = json!({"model":"gpt-test", "stream":false, "max_output_tokens":100, "input":[], "temperature":0.5}).to_string().into_bytes();
    assert!(request::body(&resolved, asb_core::UpstreamProtocol::Responses, &mut body).unwrap());
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["stream"], true);
    assert_eq!(body["store"], false);
    assert!(body.get("max_output_tokens").is_none());
    assert!(body.get("temperature").is_none());
    assert_eq!(body["instructions"], "");
    let mut other = resolved.clone();
    other.account_id = "bob".into();
    assert_ne!(
        request::continuation_key([1; 32], &resolved),
        request::continuation_key([1; 32], &other)
    );
}

#[test]
fn forced_sse_completion_requires_a_real_terminal_snapshot() {
    let bytes = b"event: response.completed\r\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[]}}\r\n\r\ndata: [DONE]\r\n\r\n";
    let response: serde_json::Value =
        serde_json::from_slice(&request::completed_response(bytes).unwrap()).unwrap();
    assert_eq!(response["status"], "completed");
    assert!(request::completed_response(b"data: [DONE]\n\n").is_err());
    assert!(request::completed_response(b"data: {\"type\":\"error\"}\n\n").is_err());
    assert!(request::completed_response(&[bytes.as_slice(), bytes.as_slice()].concat()).is_err());
}

#[test]
fn explicit_user_agents_override_managed_defaults_but_never_the_selected_credentials() {
    let account = ResolvedAccount {
        provider: ClaudeAuthProvider::GithubCopilot,
        account_id: "42".into(),
        access_token: "managed-token".into(),
        expires_at_ms: 1,
        endpoint: "https://api.githubcopilot.com".into(),
        upstream_account_id: None,
    };
    let options = ProviderConnectionOptions {
        custom_user_agent: Some("ASB custom client".into()),
        local_proxy_request_overrides: Some(asb_core::contracts::LocalProxyRequestOverrides {
            headers: std::collections::BTreeMap::from([(
                "authorization".into(),
                "Bearer wrong-account".into(),
            )]),
            body: serde_json::Value::Null,
        }),
        ..Default::default()
    };
    let mut headers = reqwest::header::HeaderMap::new();
    request::request_headers(
        &account,
        &mut headers,
        br#"{"messages":[{"role":"user","content":"hello"}]}"#,
    );
    crate::upstream_overrides::apply_header_overrides(&mut headers, &options);
    assert_eq!(headers["user-agent"], "ASB custom client");
    assert_eq!(headers["authorization"], "Bearer managed-token");
    assert_eq!(headers["x-initiator"], "user");
}
