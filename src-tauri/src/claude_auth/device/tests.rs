use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use std::thread::JoinHandle;

const NOW: i64 = 1_700_000_000_000;

fn endpoints(replies: Vec<(&'static str, u16, Value)>) -> (DeviceEndpoints, JoinHandle<()>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr());
    let task = std::thread::spawn(move || {
        for (path, status, body) in replies {
            let request = server
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .expect("expected login request");
            assert_eq!(request.url(), path);
            request
                .respond(
                    tiny_http::Response::from_string(body.to_string())
                        .with_status_code(status)
                        .with_header(
                            tiny_http::Header::from_bytes("content-type", "application/json")
                                .unwrap(),
                        ),
                )
                .unwrap();
        }
    });
    (
        DeviceEndpoints {
            start: format!("{base}/start"),
            poll: format!("{base}/poll"),
            token: format!("{base}/token"),
            user: format!("{base}/user"),
            verification: "https://login.example/device".into(),
            client_id: "test-client".into(),
            scope: "test".into(),
        },
        task,
    )
}

fn input(provider: ClaudeAuthProvider) -> ClaudeLoginRequest {
    ClaudeLoginRequest {
        provider,
        label: "本地测试账号".into(),
        github_domain: None,
        target_account_id: None,
        make_default: true,
    }
}

fn code() -> Value {
    json!({"device_code":"never-send-this-to-renderer", "user_code":"ABCD-EFGH", "verification_uri":"https://login.example/device", "expires_in":600, "interval":2})
}

fn tokens(provider: ClaudeAuthProvider) -> Value {
    let claims = if provider == ClaudeAuthProvider::CodexOauth {
        json!({"sub":"chatgpt-user", "https://api.openai.com/auth":{"chatgpt_account_id":"workspace-fixture"}})
    } else {
        json!({"sub":"xai-user","email":"fixture@example.test"})
    };
    let token = format!("e30.{}.sig", URL_SAFE_NO_PAD.encode(claims.to_string()));
    json!({"access_token":token,"refresh_token":"isolated-refresh","expires_in":3600})
}

#[test]
fn copilot_device_flow_obeys_poll_interval_slowdown_and_saves_only_application_state() {
    let dir = tempfile::tempdir().unwrap();
    let auth = ClaudeAuth::new(dir.path());
    let (endpoints, task) = endpoints(vec![
        ("/start", 200, code()),
        ("/poll", 200, json!({"error":"authorization_pending"})),
        ("/poll", 200, json!({"error":"slow_down"})),
        ("/poll", 200, json!({"access_token":"github-test-token"})),
        ("/user", 200, json!({"id":42,"login":"fixture"})),
    ]);
    let start = auth
        .start_login_at(input(ClaudeAuthProvider::GithubCopilot), endpoints, NOW)
        .unwrap();
    assert!(!serde_json::to_string(&start)
        .unwrap()
        .contains("never-send-this"));
    assert_eq!(
        auth.poll_login_at(&start.session_id, NOW + 1)
            .unwrap()
            .phase,
        "pending"
    );
    assert_eq!(
        auth.poll_login_at(&start.session_id, NOW + 2000)
            .unwrap()
            .phase,
        "pending"
    );
    assert_eq!(
        auth.poll_login_at(&start.session_id, NOW + 4000)
            .unwrap()
            .interval_seconds,
        7
    );
    assert_eq!(
        auth.poll_login_at(&start.session_id, NOW + 5000)
            .unwrap()
            .phase,
        "pending"
    );
    let done = auth.poll_login_at(&start.session_id, NOW + 11000).unwrap();
    assert_eq!(done.phase, "completed");
    assert_eq!(done.account_id.as_deref(), Some("42"));
    assert!(auth.poll_login_at(&start.session_id, NOW + 12000).is_err());
    assert_eq!(auth.view().unwrap().accounts.len(), 1);
    assert!(store::path(dir.path()).exists());
    assert!(!dir.path().join("auth.json").exists());
    task.join().unwrap();
}

#[test]
fn xai_pending_http_errors_cancel_and_denial_never_commit_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let auth = ClaudeAuth::new(dir.path());
    let (endpoints, task) = endpoints(vec![
        ("/start", 200, code()),
        ("/poll", 400, json!({"error":"authorization_pending"})),
        ("/poll", 400, json!({"error":"access_denied"})),
    ]);
    let start = auth
        .start_login_at(input(ClaudeAuthProvider::XaiOauth), endpoints, NOW)
        .unwrap();
    assert_eq!(
        auth.poll_login_at(&start.session_id, NOW + 2000)
            .unwrap()
            .phase,
        "pending"
    );
    assert!(auth.poll_login_at(&start.session_id, NOW + 4000).is_err());
    auth.cancel_login(&start.session_id).unwrap();
    assert!(auth.poll_login_at(&start.session_id, NOW + 8000).is_err());
    assert!(!store::path(dir.path()).exists());
    task.join().unwrap();
}

#[test]
fn claude_chatgpt_device_exchange_preserves_workspace_without_writing_codex_auth() {
    let dir = tempfile::tempdir().unwrap();
    let auth = ClaudeAuth::new(dir.path());
    let (endpoints, task) = endpoints(vec![
        (
            "/start",
            200,
            json!({"device_auth_id":"private-id","user_code":"USER-CODE","interval":"2"}),
        ),
        ("/poll", 404, Value::Null),
        (
            "/poll",
            200,
            json!({"authorization_code":"private-code","code_verifier":"private-verifier"}),
        ),
        ("/token", 200, tokens(ClaudeAuthProvider::CodexOauth)),
    ]);
    let start = auth
        .start_login_at(input(ClaudeAuthProvider::CodexOauth), endpoints, NOW)
        .unwrap();
    assert_eq!(
        auth.poll_login_at(&start.session_id, NOW + 2000)
            .unwrap()
            .phase,
        "pending"
    );
    assert_eq!(
        auth.poll_login_at(&start.session_id, NOW + 4000)
            .unwrap()
            .phase,
        "completed"
    );
    let view = auth.view().unwrap();
    assert_eq!(
        view.accounts[0].upstream_account_id.as_deref(),
        Some("workspace-fixture")
    );
    assert!(!dir.path().join("auth.json").exists());
    task.join().unwrap();
}

#[test]
fn completed_login_tokens_survive_a_store_failure_for_an_explicit_retry() {
    let dir = tempfile::tempdir().unwrap();
    let auth = ClaudeAuth::new(dir.path());
    let (endpoints, task) = endpoints(vec![
        ("/start", 200, code()),
        ("/poll", 200, tokens(ClaudeAuthProvider::XaiOauth)),
    ]);
    let start = auth
        .start_login_at(input(ClaudeAuthProvider::XaiOauth), endpoints, NOW)
        .unwrap();
    std::fs::create_dir(store::path(dir.path())).unwrap();
    assert!(auth.poll_login_at(&start.session_id, NOW + 2000).is_err());
    std::fs::remove_dir(store::path(dir.path())).unwrap();
    assert_eq!(
        auth.poll_login_at(&start.session_id, NOW + 4000)
            .unwrap()
            .phase,
        "completed"
    );
    task.join().unwrap();
}

#[test]
fn expiration_and_cancellation_prevent_a_late_exchange() {
    for expired in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let auth = ClaudeAuth::new(dir.path());
        let (endpoints, task) = endpoints(vec![("/start", 200, code())]);
        let start = auth
            .start_login_at(input(ClaudeAuthProvider::XaiOauth), endpoints, NOW)
            .unwrap();
        if !expired {
            auth.cancel_login(&start.session_id).unwrap();
        }
        assert!(auth
            .poll_login_at(&start.session_id, NOW + 600_000)
            .is_err());
        assert!(!store::path(dir.path()).exists());
        task.join().unwrap();
    }
}

#[test]
fn cancelling_an_inflight_device_poll_prevents_a_late_authorization_from_being_saved() {
    use std::sync::{mpsc, Arc};
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    let auth = Arc::new(ClaudeAuth::new(dir.path()));
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr());
    let (entered, entered_rx) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let task = std::thread::spawn(move || {
        let start = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        start
            .respond(tiny_http::Response::from_string(code().to_string()))
            .unwrap();
        let poll = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        entered.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        poll.respond(tiny_http::Response::from_string(
            tokens(ClaudeAuthProvider::XaiOauth).to_string(),
        ))
        .unwrap();
    });
    let start = auth
        .start_login_at(
            input(ClaudeAuthProvider::XaiOauth),
            DeviceEndpoints {
                start: format!("{base}/start"),
                poll: format!("{base}/poll"),
                token: format!("{base}/token"),
                user: format!("{base}/user"),
                verification: "https://login.example/device".into(),
                client_id: "fixture".into(),
                scope: "fixture".into(),
            },
            NOW,
        )
        .unwrap();
    let owner = auth.clone();
    let id = start.session_id.clone();
    let poll = std::thread::spawn(move || owner.poll_login_at(&id, NOW + 2000));
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    auth.cancel_login(&start.session_id).unwrap();
    release.send(()).unwrap();
    assert!(poll.join().unwrap().unwrap_err().contains("取消"));
    assert!(!store::path(dir.path()).exists());
    task.join().unwrap();
}

#[test]
fn a_pending_login_does_not_replace_a_newer_explicit_default_account_choice() {
    let dir = tempfile::tempdir().unwrap();
    let auth = ClaudeAuth::new(dir.path());
    let saved = |id: &str| ClaudeAccount {
        id: id.into(),
        label: id.into(),
        provider: ClaudeAuthProvider::XaiOauth,
        access_token: format!("fake-{id}"),
        refresh_token: None,
        expires_at_ms: Some(NOW + 3_600_000),
        upstream_account_id: None,
        github_domain: None,
    };
    auth.save_account(saved("old"), &auth.view().unwrap().file_hash, true)
        .unwrap();
    let (endpoints, task) = endpoints(vec![
        ("/start", 200, code()),
        ("/poll", 200, tokens(ClaudeAuthProvider::XaiOauth)),
    ]);
    let start = auth
        .start_login_at(input(ClaudeAuthProvider::XaiOauth), endpoints, NOW)
        .unwrap();
    auth.save_account(saved("chosen-later"), &auth.view().unwrap().file_hash, true)
        .unwrap();
    let completed = auth.poll_login_at(&start.session_id, NOW + 2000).unwrap();
    assert_eq!(completed.phase, "completed");
    assert_eq!(
        auth.view()
            .unwrap()
            .accounts
            .iter()
            .find(|account| account.is_default)
            .unwrap()
            .id,
        "chosen-later"
    );
    task.join().unwrap();
}
