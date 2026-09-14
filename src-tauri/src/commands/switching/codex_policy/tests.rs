use super::*;
use crate::gateway::codex::policy::CodexGatewayPolicy;
use std::fs;

struct Fixture {
    _paths: crate::test_client_paths::ClientPathGuard,
    _directory: tempfile::TempDir,
    state: LocalState,
    gateway: GatewayController,
    id: String,
}
impl Fixture {
    fn new() -> Self {
        let paths = crate::test_client_paths::redirect_client_paths();
        let directory = tempfile::tempdir().unwrap();
        let state = LocalState::from_root(directory.path().join("state"));
        let target = state.target(asb_core::AppKind::Codex).unwrap();
        fs::write(
            &target,
            "model_provider = 'openai'\nunmanaged_key = 'keep'\n",
        )
        .unwrap();
        fs::write(target.with_file_name("auth.json"), r#"{"auth_mode":"chatgpt","tokens":{"access_token":"fixture-at","refresh_token":"fixture-rt","id_token":"fixture-id"}}"#).unwrap();
        let record = state
            .configuration()
            .create_codex_provider(super::super::transaction_tests::draft("policy-model"))
            .unwrap();
        let gateway = GatewayController::start(&state);
        Self {
            _paths: paths,
            _directory: directory,
            state,
            gateway,
            id: record.profile.id,
        }
    }
    fn policy(&self) -> CodexGatewayPolicy {
        CodexGatewayPolicy {
            takeover: true,
            enabled: true,
            provider_ids: vec![self.id.clone()],
            ..Default::default()
        }
    }
    fn target(&self) -> std::path::PathBuf {
        self.state.target(asb_core::AppKind::Codex).unwrap()
    }
    fn prepare(&self) -> preparations::Prepared {
        preparations::prepare(&self.state, &self.gateway, self.id.clone(), self.policy()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.gateway.shutdown();
    }
}

#[test]
fn codex_policy_prepare_cancel_is_pure_and_preparations_are_one_shot() {
    let fixture = Fixture::new();
    let before = fs::read(fixture.target()).unwrap();
    let auth = fs::read(fixture.target().with_file_name("auth.json")).unwrap();
    let pending = CodexPolicyPreparations::default();
    let id = pending.insert(fixture.prepare()).unwrap();
    pending.cancel(&id).unwrap();
    assert!(pending.take(&id).is_err());
    let id = pending.insert(fixture.prepare()).unwrap();
    pending.take(&id).unwrap();
    assert!(pending.take(&id).is_err());
    assert_eq!(fs::read(fixture.target()).unwrap(), before);
    assert_eq!(
        fs::read(fixture.target().with_file_name("auth.json")).unwrap(),
        auth
    );
    assert!(!policy::path(fixture.state.root()).exists());
}

#[test]
fn codex_takeover_and_stop_roundtrip_use_the_executor_and_preserve_unowned_keys() {
    let fixture = Fixture::new();
    let prepared = fixture.prepare();
    preparations::validate(&fixture.state, &fixture.gateway, &prepared).unwrap();
    let switched = commit_prepared(&fixture.state, &fixture.gateway, &prepared).unwrap();
    assert_eq!(switched.final_hash, prepared.preview.rendered_hash);
    assert!(fs::read_to_string(fixture.target())
        .unwrap()
        .contains("unmanaged_key = 'keep'"));
    assert!(fixture
        .gateway
        .has_active_route_for(asb_core::AppKind::Codex));
    assert!(!policy::pending_path(fixture.state.root()).exists());
    let policy = CodexGatewayPolicy {
        provider_ids: vec![fixture.id.clone()],
        ..Default::default()
    };
    let direct =
        preparations::prepare(&fixture.state, &fixture.gateway, fixture.id.clone(), policy)
            .unwrap();
    commit_prepared(&fixture.state, &fixture.gateway, &direct).unwrap();
    assert!(!fixture
        .gateway
        .has_active_route_for(asb_core::AppKind::Codex));
    assert!(fs::read_to_string(fixture.target())
        .unwrap()
        .contains("https://relay.example/v1"));
    assert!(!fs::read_to_string(policy::path(fixture.state.root()))
        .unwrap()
        .contains("api_key"));
}

#[test]
fn codex_policy_rejects_stale_profile_or_queue_before_native_writes() {
    let fixture = Fixture::new();
    let prepared = fixture.prepare();
    let before = fs::read(fixture.target()).unwrap();
    let (mut file, revision) = fixture
        .state
        .configuration()
        .find_codex_provider_with_revision(&fixture.id)
        .unwrap();
    file.profile.name = "Changed elsewhere".into();
    fixture
        .state
        .configuration()
        .update_codex_provider_file(file, &revision)
        .unwrap();
    assert!(preparations::validate(&fixture.state, &fixture.gateway, &prepared).is_err());
    assert_eq!(fs::read(fixture.target()).unwrap(), before);
    assert!(!policy::pending_path(fixture.state.root()).exists());
    let mut policy = fixture.policy();
    policy
        .provider_ids
        .insert(0, uuid::Uuid::new_v4().to_string());
    assert!(
        preparations::prepare(&fixture.state, &fixture.gateway, fixture.id.clone(), policy)
            .is_err()
    );
}

#[test]
fn codex_policy_recovers_a_crash_after_policy_save_without_touching_client_files() {
    let fixture = Fixture::new();
    let prepared = fixture.prepare();
    let before = fs::read(fixture.target()).unwrap();
    journal::begin(&fixture.state, &fixture.gateway, &prepared).unwrap();
    policy::save(fixture.state.root(), &prepared.policy).unwrap();
    journal::recover(&fixture.state, &fixture.gateway, false).unwrap();
    assert!(!policy::path(fixture.state.root()).exists());
    assert!(!policy::pending_path(fixture.state.root()).exists());
    assert_eq!(fs::read(fixture.target()).unwrap(), before);
}

#[test]
fn codex_policy_external_edits_require_explicit_abandon_and_remain_intact() {
    let fixture = Fixture::new();
    let prepared = fixture.prepare();
    journal::begin(&fixture.state, &fixture.gateway, &prepared).unwrap();
    policy::save(fixture.state.root(), &prepared.policy).unwrap();
    let external = "model_provider = 'openai'\nunmanaged_key = 'externally changed'\n";
    fs::write(fixture.target(), external).unwrap();
    assert!(journal::recover(&fixture.state, &fixture.gateway, false).is_err());
    journal::abandon(
        &fixture.state,
        &fixture.gateway,
        &asb_switch::sha256_hex(external),
    )
    .unwrap();
    assert_eq!(fs::read_to_string(fixture.target()).unwrap(), external);
    assert!(!policy::pending_path(fixture.state.root()).exists());
    assert!(fs::read_dir(fixture.state.root().join("codex"))
        .unwrap()
        .flatten()
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with("abandoned-policy-")));
}

#[test]
fn taken_native_codex_can_restore_its_backup_and_participate_in_port_change() {
    let fixture = Fixture::new();
    let taken = fixture.prepare();
    commit_prepared(&fixture.state, &fixture.gateway, &taken).unwrap();
    let captured = fs::read_to_string(fixture.target()).unwrap();
    let direct = preparations::prepare(
        &fixture.state,
        &fixture.gateway,
        fixture.id.clone(),
        CodexGatewayPolicy::default(),
    )
    .unwrap();
    let stopped = commit_prepared(&fixture.state, &fixture.gateway, &direct).unwrap();
    super::super::backups::run_restore(&fixture.state, &fixture.gateway, &stopped.backup).unwrap();
    assert_eq!(fs::read_to_string(fixture.target()).unwrap(), captured);
    assert!(fixture
        .gateway
        .has_active_route_for(asb_core::AppKind::Codex));
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let pending = crate::gateway::PortChangePreparations::default();
    let preview =
        crate::gateway::port_change::prepare(&fixture.gateway, &fixture.state, &pending, port)
            .unwrap();
    assert!(preview
        .clients
        .iter()
        .any(|client| client.app == asb_core::AppKind::Codex));
    pending.cancel(&preview.preparation_id).unwrap();
    assert_eq!(fs::read_to_string(fixture.target()).unwrap(), captured);
}

#[test]
fn codex_policy_preview_is_bound_to_the_exact_native_config_path() {
    let fixture = Fixture::new();
    let mut prepared = fixture.prepare();
    let before = fs::read(fixture.target()).unwrap();
    prepared.preview.preview.target = fixture
        .state
        .root()
        .join("another-root/config.toml")
        .to_string_lossy()
        .into_owned();
    assert!(preparations::validate(&fixture.state, &fixture.gateway, &prepared).is_err());
    assert_eq!(fs::read(fixture.target()).unwrap(), before);
    assert!(!policy::pending_path(fixture.state.root()).exists());
}
