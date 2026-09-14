use super::{backups, plan, transaction, transaction_tests::draft};
use crate::{gateway::GatewayController, local_state::LocalState};
use asb_core::{AppKind, MatchStatus, ProviderRecord, WriteOperation};
use asb_switch::{FsIo, SwitchOutcome, SwitchRequest};
use std::{fs, panic::AssertUnwindSafe, path::PathBuf};

const AUTH: &str = r#"{"auth_mode":"chatgpt","tokens":{"access_token":"at","refresh_token":"rt","id_token":"id"}}"#;

struct Fixture {
    _paths: crate::test_client_paths::ClientPathGuard,
    _directory: tempfile::TempDir,
    state: LocalState,
    gateway: GatewayController,
}

impl Fixture {
    fn new() -> Self {
        let paths = crate::test_client_paths::redirect_client_paths();
        let directory = tempfile::tempdir().unwrap();
        let state = LocalState::from_root(directory.path().join("state"));
        let target = state.target(AppKind::Codex).unwrap();
        fs::write(&target, "model_provider = 'openai'\nmodel = 'before'\n").unwrap();
        fs::write(target.with_file_name("auth.json"), AUTH).unwrap();
        let gateway = GatewayController::start(&state);
        Self {
            _paths: paths,
            _directory: directory,
            state,
            gateway,
        }
    }

    fn target(&self) -> PathBuf {
        self.state.target(AppKind::Codex).unwrap()
    }

    fn official(&self) -> ProviderRecord {
        self.state
            .configuration()
            .ensure_codex_official_record()
            .unwrap()
            .0
    }

    fn activate(&self, id: &str) -> SwitchOutcome {
        let projection = plan::build_plan(&self.state, &self.gateway, id).unwrap();
        let preview = plan::preview_projection(&self.state, &projection).unwrap();
        plan::execute_projection(
            &self.state,
            &self.gateway,
            &projection,
            &preview.content_hash,
            &preview.rendered_hash,
        )
        .unwrap()
    }

    fn catalog_path(&self) -> PathBuf {
        let text = fs::read_to_string(self.target()).unwrap();
        let document = text.parse::<toml_edit::DocumentMut>().unwrap();
        self.target()
            .parent()
            .unwrap()
            .join(document["model_catalog_json"].as_str().unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.gateway.shutdown();
    }
}

#[test]
fn restore_recreates_codex_catalog_after_switching_to_official() {
    let fixture = Fixture::new();
    let custom = fixture
        .state
        .configuration()
        .create_codex_provider(draft("model-a"))
        .unwrap();
    fixture.activate(&custom.profile.id);
    let before = fs::read_to_string(fixture.target()).unwrap();
    let catalog_path = fixture.catalog_path();
    let catalog = fs::read_to_string(&catalog_path).unwrap();
    let official = fixture.official();
    let switched = fixture.activate(&official.profile.id);
    assert!(!catalog_path.exists());

    backups::run_restore(&fixture.state, &fixture.gateway, &switched.backup).unwrap();

    assert_eq!(fs::read_to_string(fixture.target()).unwrap(), before);
    assert_eq!(fs::read_to_string(catalog_path).unwrap(), catalog);
    let status = crate::commands::config_status_report(&fixture.state, &fixture.gateway)
        .unwrap()
        .into_iter()
        .find(|status| status.app == AppKind::Codex)
        .unwrap();
    assert_eq!(status.active_profile_id, Some(custom.profile.id));
    let restored_auth: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(fixture.target().with_file_name("auth.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(restored_auth["OPENAI_API_KEY"], "fixture-key");
    assert_eq!(restored_auth["tokens"]["access_token"], "at");
}

#[test]
fn restoring_a_catalog_never_overwrites_an_external_revision() {
    let fixture = Fixture::new();
    let custom = fixture
        .state
        .configuration()
        .create_codex_provider(draft("model-a"))
        .unwrap();
    fixture.activate(&custom.profile.id);
    let catalog_path = fixture.catalog_path();
    let switched = fixture.activate(&fixture.official().profile.id);
    let official_config = fs::read_to_string(fixture.target()).unwrap();
    fs::write(&catalog_path, r#"{"models":[],"external":true}"#).unwrap();

    assert!(backups::run_restore(&fixture.state, &fixture.gateway, &switched.backup).is_err());
    assert_eq!(
        fs::read_to_string(fixture.target()).unwrap(),
        official_config
    );
    assert_eq!(
        fs::read_to_string(catalog_path).unwrap(),
        r#"{"models":[],"external":true}"#
    );
}

#[test]
fn official_switch_records_and_reports_the_selected_profile() {
    let fixture = Fixture::new();
    let official = fixture.official();
    fixture.activate(&official.profile.id);
    let history = fixture
        .state
        .configuration()
        .latest_config_write(AppKind::Codex)
        .unwrap()
        .unwrap();
    assert_eq!(
        history.profile_id.as_deref(),
        Some(official.profile.id.as_str())
    );
    assert_eq!(history.operation, WriteOperation::Projection);
    let status = crate::commands::config_status_report(&fixture.state, &fixture.gateway).unwrap();
    let codex = status
        .iter()
        .find(|status| status.app == AppKind::Codex)
        .unwrap();
    assert_eq!(
        codex.active_profile_id.as_deref(),
        Some(official.profile.id.as_str())
    );
    assert!(
        matches!(&codex.match_status, MatchStatus::MatchesProfile { profile_id, .. } if profile_id == &official.profile.id)
    );
}

#[test]
fn interrupted_official_switch_recovers_the_same_profile_identity() {
    let fixture = Fixture::new();
    let official = fixture.official();
    let projection =
        plan::build_plan(&fixture.state, &fixture.gateway, &official.profile.id).unwrap();
    let preview = plan::preview_projection(&fixture.state, &projection).unwrap();
    transaction::begin(
        &fixture.state,
        &fixture.gateway,
        AppKind::Codex,
        Some(&official.profile.id),
        &preview.rendered_hash,
        true,
        None,
    )
    .unwrap();
    let target = fixture.target();
    let backup_dir = fixture.state.backup_dir();
    let crashed = std::panic::catch_unwind(AssertUnwindSafe(|| {
        asb_switch::execute(
            &FsIo,
            &SwitchRequest {
                target: &target,
                plan: &projection.plan,
                backup_dir: &backup_dir,
                expected_hash: &preview.content_hash,
                expected_rendered_hash: &preview.rendered_hash,
            },
            |_| panic!("injected interruption before official route commit"),
        )
    }));
    assert!(crashed.is_err());
    asb_switch::release(&FsIo, &target).unwrap();

    transaction::recover(&fixture.state, &fixture.gateway).unwrap();

    let history = fixture
        .state
        .configuration()
        .latest_config_write(AppKind::Codex)
        .unwrap()
        .unwrap();
    assert_eq!(
        history.profile_id.as_deref(),
        Some(official.profile.id.as_str())
    );
    assert_eq!(history.operation, WriteOperation::Projection);
    assert!(
        asb_switch::pending_config_write(&FsIo, &backup_dir, AppKind::Codex)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fs::read_to_string(target.with_file_name("auth.json")).unwrap(),
        AUTH
    );
}
