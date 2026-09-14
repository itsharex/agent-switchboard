use super::*;
use asb_core::contracts::{ProviderDraft, RouteMode, UpstreamProtocol};
use serde_json::{json, Value};

struct Fixture {
    _paths: crate::test_client_paths::ClientPathGuard,
    _dir: tempfile::TempDir,
    local: LocalState,
    gateway: GatewayController,
}

impl Fixture {
    fn new() -> Self {
        let paths = crate::test_client_paths::redirect_client_paths();
        let dir = tempfile::tempdir().unwrap();
        let local = LocalState::from_root(dir.path().join("state"));
        let target = local.target(AppKind::Claude).unwrap();
        fs::write(&target, json!({"model":"original", "env":{"ANTHROPIC_BASE_URL":"https://original.example", "ANTHROPIC_API_KEY":"fixture-original-key"}, "language":"en"}).to_string()).unwrap();
        let gateway = GatewayController::start(&local);
        Self {
            _paths: paths,
            _dir: dir,
            local,
            gateway,
        }
    }
    fn activate(&self, name: &str) -> String {
        let profile = self
            .local
            .configuration()
            .create_provider(ProviderDraft {
                app: AppKind::Claude,
                route_mode: RouteMode::Custom,
                name: name.into(),
                model: Some(name.into()),
                base_url: Some("https://relay.example/v1".into()),
                api_key: format!("fixture-{name}"),
                authentication: None,
                connection: Default::default(),
                upstream_protocol: Some(UpstreamProtocol::ChatCompletions),
                responses_options: None,
                max_output_tokens: None.into(),
                model_options: None,
                parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
                notes: None,
                website_url: None,
                usage_query: None,
                official_quota_refresh_interval_minutes: None,
            })
            .unwrap();
        let projection =
            super::super::plan::build_plan(&self.local, &self.gateway, &profile.profile.id)
                .unwrap();
        let preview = super::super::plan::preview_projection(&self.local, &projection).unwrap();
        super::super::plan::execute_projection(
            &self.local,
            &self.gateway,
            &projection,
            &preview.content_hash,
            &preview.rendered_hash,
        )
        .unwrap();
        profile.profile.id
    }
    fn target(&self) -> PathBuf {
        self.local.target(AppKind::Claude).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.gateway.shutdown();
    }
}

#[test]
fn stop_restores_original_connection_after_multiple_hot_switches_and_preserves_external_preferences(
) {
    let fixture = Fixture::new();
    fixture.activate("first");
    fixture.activate("second");
    let mut current: Value =
        serde_json::from_str(&fs::read_to_string(fixture.target()).unwrap()).unwrap();
    current["language"] = json!("zh");
    current["env"]["HOST"] = json!("keep");
    fs::write(fixture.target(), current.to_string()).unwrap();
    let (preview, _) = candidate(&fixture.local, &fixture.gateway, None).unwrap();
    let wire = serde_json::to_string(&preview).unwrap();
    assert!(!wire.contains("fixture-original-key"));
    stop(&fixture.local, &fixture.gateway, &preview).unwrap();
    let restored: Value =
        serde_json::from_str(&fs::read_to_string(fixture.target()).unwrap()).unwrap();
    assert_eq!(restored["model"], "original");
    assert_eq!(restored["language"], "zh");
    assert_eq!(restored["env"]["HOST"], "keep");
    assert_eq!(restored["env"]["ANTHROPIC_API_KEY"], "fixture-original-key");
    assert!(!fixture.gateway.has_active_route_for(AppKind::Claude));
    assert_eq!(
        fixture
            .local
            .configuration()
            .latest_config_write(AppKind::Claude)
            .unwrap()
            .unwrap()
            .operation,
        WriteOperation::Restore
    );
}

#[test]
fn stale_stop_preview_never_overwrites_a_later_external_edit() {
    let fixture = Fixture::new();
    fixture.activate("first");
    let (preview, _) = candidate(&fixture.local, &fixture.gateway, None).unwrap();
    let edited = fs::read_to_string(fixture.target()).unwrap() + "\n ";
    fs::write(fixture.target(), &edited).unwrap();
    assert!(stop(&fixture.local, &fixture.gateway, &preview).is_err());
    assert_eq!(fs::read_to_string(fixture.target()).unwrap(), edited);
    assert!(fixture.gateway.has_active_route_for(AppKind::Claude));
}

#[test]
fn stop_rejects_tampered_backup_and_preserves_live_config() {
    let fixture = Fixture::new();
    fixture.activate("first");
    let (preview, _) = candidate(&fixture.local, &fixture.gateway, None).unwrap();
    let backup = find_backup(&fixture.local, &preview.backup_id).unwrap();
    fs::write(backup.backup_path, "{}").unwrap();
    let before = fs::read_to_string(fixture.target()).unwrap();
    assert!(stop(&fixture.local, &fixture.gateway, &preview).is_err());
    assert_eq!(fs::read_to_string(fixture.target()).unwrap(), before);
}

#[test]
fn restoring_claude_backup_selects_its_exact_revision_despite_stable_capability() {
    let fixture = Fixture::new();
    let first = fixture.activate("first");
    let before = fs::read_to_string(fixture.target()).unwrap();
    let second = fixture.activate("second");
    let after = fs::read_to_string(fixture.target()).unwrap();
    let before_json: Value = serde_json::from_str(&before).unwrap();
    let after_json: Value = serde_json::from_str(&after).unwrap();
    assert_eq!(
        before_json["env"]["ANTHROPIC_AUTH_TOKEN"],
        after_json["env"]["ANTHROPIC_AUTH_TOKEN"]
    );
    assert_ne!(first, second);
    assert_ne!(
        before_json["env"]["ASB_CLAUDE_ROUTE_REVISION"],
        after_json["env"]["ASB_CLAUDE_ROUTE_REVISION"]
    );
    let prepared = fixture
        .gateway
        .prepare_restored(&fixture.local, AppKind::Claude, &before)
        .unwrap();
    fs::write(fixture.target(), &prepared).unwrap();
    fixture
        .gateway
        .reconcile_restored(&fixture.local, AppKind::Claude, || Ok(()))
        .unwrap();
    assert_eq!(
        fixture
            .gateway
            .active_profile_id(AppKind::Claude, &prepared)
            .unwrap(),
        Some(first)
    );
}
