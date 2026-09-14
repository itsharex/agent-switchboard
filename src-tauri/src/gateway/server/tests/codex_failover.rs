use super::*;
use crate::gateway::codex::policy::{self, CodexGatewayPolicy};
use tiny_http::StatusCode;

struct Fixture {
    _paths: crate::test_client_paths::ClientPathGuard,
    _directory: tempfile::TempDir,
    state: LocalState,
    gateway: GatewayController,
    primary: CodexProviderFile,
    backup: CodexProviderFile,
    url: String,
    policy: CodexGatewayPolicy,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.gateway.shutdown();
    }
}
impl Fixture {
    fn new(primary: &str, backup: &str, enabled: bool) -> Self {
        let paths = crate::test_client_paths::redirect_client_paths();
        let directory = tempfile::tempdir().unwrap();
        let state = LocalState::from_root(directory.path().join("state"));
        let primary = sandbox_codex_file(
            &state,
            "Codex primary",
            primary.into(),
            "codex-primary-key".into(),
            CodexUpstream::ChatCompletions,
        );
        let mut backup = sandbox_codex_file(
            &state,
            "Codex backup",
            backup.into(),
            "codex-backup-key".into(),
            CodexUpstream::ChatCompletions,
        );
        let (_, revision) = state
            .configuration()
            .find_codex_provider_with_revision(&backup.profile.id)
            .unwrap();
        backup.profile.model_routes[0].upstream_model = "backup-wire-model".into();
        state
            .configuration()
            .update_codex_provider_file(backup.clone(), &revision)
            .unwrap();
        let policy = CodexGatewayPolicy {
            takeover: true,
            enabled,
            provider_ids: vec![primary.profile.id.clone(), backup.profile.id.clone()],
            max_retries: 1,
            ..Default::default()
        };
        policy::save(state.root(), &policy).unwrap();
        let gateway = GatewayController::start(&state);
        let projection = gateway
            .project_codex(&primary, default_client_settings(AppKind::Codex))
            .unwrap();
        let url = codex_endpoint(&projection);
        gateway.commit(&projection, || Ok(())).unwrap();
        Self {
            _paths: paths,
            _directory: directory,
            state,
            gateway,
            primary,
            backup,
            url,
            policy,
        }
    }
    fn send(&self) -> reqwest::blocking::Response {
        Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap()
            .post(&self.url).header(CONTENT_TYPE, "application/json")
            .body(json!({"model":"sandbox-model", "input":[{"role":"user","content":"hello"}], "stream":false}).to_string())
            .send().unwrap()
    }
}
fn reply_once(server: Server, status: u16, body: Value) -> thread::JoinHandle<(Value, String)> {
    thread::spawn(move || {
        let mut request = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .expect("upstream request");
        let authorization = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("Authorization"))
            .map(|header| header.value.to_string())
            .unwrap_or_default();
        let mut text = String::new();
        request.as_reader().read_to_string(&mut text).unwrap();
        request
            .respond(
                Response::from_string(body.to_string())
                    .with_status_code(StatusCode(status))
                    .with_header(content_type("application/json")),
            )
            .unwrap();
        (serde_json::from_str(&text).unwrap(), authorization)
    })
}
fn success() -> Value {
    json!({"id":"codex-fixture", "model":"backup-wire-model", "choices":[{
    "index":0,"message":{"role":"assistant","content":"recovered"},"finish_reason":"stop"}],
    "usage":{"prompt_tokens":10,"completion_tokens":3,"total_tokens":13}})
}

#[test]
fn codex_failover_uses_queue_order_and_each_candidates_key_and_model() {
    let primary = Server::http(("127.0.0.1", 0)).unwrap();
    let backup = Server::http(("127.0.0.1", 0)).unwrap();
    let fixture = Fixture::new(&endpoint(&primary), &endpoint(&backup), true);
    let first = reply_once(primary, 429, json!({"error":{"message":"capacity"}}));
    let second = reply_once(backup, 200, success());
    let response = fixture.send();
    assert_eq!(response.status().as_u16(), 200);
    let body: Value = serde_json::from_str(&response.text().unwrap()).unwrap();
    assert!(body.to_string().contains("recovered"));
    let (a, key_a) = first.join().unwrap();
    let (b, key_b) = second.join().unwrap();
    assert_eq!(key_a, "Bearer codex-primary-key");
    assert_eq!(key_b, "Bearer codex-backup-key");
    assert_eq!(a["model"], "sandbox-model");
    assert_eq!(b["model"], "backup-wire-model");
    let ledger = crate::codex_metering::CodexRequestLedger::new(fixture.state.root());
    let record = ledger.page(&Default::default(),0,10).unwrap().records.remove(0);
    assert_eq!(record.profile_id.as_deref(),Some(fixture.backup.profile.id.as_str()));
    assert_eq!(record.request_model.as_deref(),Some("sandbox-model"));
    assert_eq!(record.mapped_model.as_deref(),Some("backup-wire-model"));
    assert_eq!(record.input_tokens,Some(10));
    assert_eq!(record.attempts.len(),2);
    assert_eq!(record.attempts[0].status,Some(429));
    assert_eq!(record.attempts[1].status,Some(200));
    let snapshot = fixture.gateway.observe(&fixture.state);
    assert_eq!(
        snapshot
            .metrics
            .samples
            .last()
            .unwrap()
            .profile_id
            .as_deref(),
        Some(fixture.backup.profile.id.as_str())
    );
    assert_eq!(
        fixture
            .gateway
            .active_profile_id_for(AppKind::Codex)
            .as_deref(),
        Some(fixture.primary.profile.id.as_str())
    );
}

#[test]
fn codex_failover_disabled_keeps_queue_but_never_sends_to_backup() {
    let primary = Server::http(("127.0.0.1", 0)).unwrap();
    let backup = Server::http(("127.0.0.1", 0)).unwrap();
    let fixture = Fixture::new(&endpoint(&primary), &endpoint(&backup), false);
    let first = reply_once(primary, 503, json!({"error":{"message":"busy"}}));
    assert_eq!(fixture.send().status().as_u16(), 503);
    first.join().unwrap();
    assert!(backup
        .recv_timeout(Duration::from_millis(120))
        .unwrap()
        .is_none());
    assert_eq!(
        policy::load(fixture.state.root())
            .unwrap()
            .0
            .provider_ids
            .len(),
        2
    );
}

#[test]
fn codex_failover_does_not_retry_client_parameter_errors() {
    let primary = Server::http(("127.0.0.1", 0)).unwrap();
    let backup = Server::http(("127.0.0.1", 0)).unwrap();
    let fixture = Fixture::new(&endpoint(&primary), &endpoint(&backup), true);
    let first = reply_once(primary, 400, json!({"error":{"message":"bad parameters"}}));
    assert_eq!(fixture.send().status().as_u16(), 400);
    first.join().unwrap();
    assert!(backup
        .recv_timeout(Duration::from_millis(120))
        .unwrap()
        .is_none());
    let health = fixture.gateway.inner.health_for(
        &fixture
            .gateway
            .route_for_codex_file(&fixture.primary)
            .unwrap(),
    );
    assert_eq!(health.consecutive_failures(), 0);
}

#[test]
fn codex_native_responses_can_choose_direct_or_takeover_without_mutating_the_profile() {
    let primary = Server::http(("127.0.0.1", 0)).unwrap();
    let backup = Server::http(("127.0.0.1", 0)).unwrap();
    let fixture = Fixture::new(&endpoint(&primary), &endpoint(&backup), false);
    let mut file = fixture.primary.clone();
    file.profile.upstream = CodexUpstream::Responses;
    file.profile.capabilities.chat_reasoning = CodexChatReasoning::Unsupported;
    file.profile.connection.custom_user_agent = None;
    file.profile.route_mode = CodexRouteMode::Direct;
    let direct = fixture
        .gateway
        .project_codex_with_policy(
            &file,
            default_client_settings(AppKind::Codex),
            &CodexGatewayPolicy::default(),
        )
        .unwrap();
    assert!(!direct.plan.is_gateway());
    let taken = fixture
        .gateway
        .project_codex_with_policy(
            &file,
            default_client_settings(AppKind::Codex),
            &fixture.policy,
        )
        .unwrap();
    assert!(taken.plan.is_gateway());
    assert_eq!(file.profile.route_mode, CodexRouteMode::Direct);
}

#[test]
fn codex_health_survives_restart_and_reset_invalidates_inflight_callbacks() {
    let primary = Server::http(("127.0.0.1", 0)).unwrap();
    let backup = Server::http(("127.0.0.1", 0)).unwrap();
    let fixture = Fixture::new(&endpoint(&primary), &endpoint(&backup), true);
    let route = fixture
        .gateway
        .route_for_codex_file(&fixture.primary)
        .unwrap();
    let store = fixture.gateway.inner.codex_health.clone();
    let config = crate::gateway::ProviderHealthConfig::new(1, Duration::from_secs(300));
    let health = store.health_for(&route, config);
    health.try_acquire().unwrap().record_failure();
    assert_eq!(health.state(), crate::gateway::ProviderHealthState::Open);
    let reloaded = Arc::new(crate::gateway::codex::health::CodexHealthStore::new(
        fixture.state.root(),
    ));
    assert_eq!(
        reloaded.health_for(&route, config).state(),
        crate::gateway::ProviderHealthState::Open
    );
    store.reset(&route).unwrap();
    let old = store.health_for(&route, config);
    let in_flight = old.try_acquire().unwrap();
    store.reset(&route).unwrap();
    in_flight.record_failure();
    assert_eq!(
        store.health_for(&route, config).state(),
        crate::gateway::ProviderHealthState::Closed
    );
    let raw = fs::read_to_string(fixture.state.root().join("codex/health.json")).unwrap();
    assert!(!raw.contains("codex-primary-key"));
    assert!(!raw.contains("http://"));
}
