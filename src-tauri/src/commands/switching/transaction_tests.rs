use super::{plan, profile_rollback, transaction};
use crate::{gateway::GatewayController, local_state::LocalState};
use asb_core::{AppKind, ProviderDraft, ProviderRecord, RouteMode};
use asb_switch::FsIo;
use std::{fs, panic::AssertUnwindSafe};

const BEFORE: &str = "model_provider = 'openai'\nmodel = 'before-model'\n";

fn draft(revision: &str) -> ProviderDraft {
    ProviderDraft {
        app: AppKind::Codex,
        route_mode: RouteMode::Official,
        name: format!("Official fixture {revision}"),
        base_url: None,
        api_key: String::new(),
        upstream_protocol: None,
        responses_options: None,
        model: None,
        model_options: None,
        max_output_tokens: None.into(),
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Codex),
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

fn crash_after_config_write(state: &LocalState, gateway: &GatewayController) -> ProviderRecord {
    let record = state
        .configuration()
        .create_provider(draft("after-model"))
        .unwrap();
    let target = state.target(AppKind::Codex).unwrap();
    fs::write(&target, BEFORE).unwrap();
    fs::write(target.with_file_name("auth.json"), "account-owned-by-codex").unwrap();
    let projection = plan::build_plan_for_profile(state, gateway, record.profile.clone()).unwrap();
    let preview = plan::preview_projection(state, &projection).unwrap();
    assert_ne!(
        preview.content_hash, preview.rendered_hash,
        "official projection clears the old model override"
    );
    transaction::begin(
        state,
        gateway,
        AppKind::Codex,
        Some(&record.profile.id),
        &preview.rendered_hash,
        true,
    )
    .unwrap();
    let crashed = std::panic::catch_unwind(AssertUnwindSafe(|| {
        asb_switch::execute(
            &FsIo,
            &asb_switch::SwitchRequest {
                target: &target,
                plan: &projection.plan,
                backup_dir: &state.backup_dir(),
                expected_hash: &preview.content_hash,
                expected_rendered_hash: &preview.rendered_hash,
            },
            |_| panic!("injected crash before application commit"),
        )
    }));
    assert!(crashed.is_err());
    asb_switch::release(&FsIo, &target).unwrap();
    record
}

#[test]
fn transaction_recovery_finishes_app_commit_once_without_touching_login() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&state);
    let record = crash_after_config_write(&state, &gateway);
    transaction::recover(&state, &gateway).unwrap();
    let history = state
        .configuration()
        .latest_config_write(AppKind::Codex)
        .unwrap()
        .unwrap();
    assert_eq!(
        history.profile_id.as_deref(),
        Some(record.profile.id.as_str())
    );
    transaction::recover(&state, &gateway).unwrap();
    assert_eq!(
        state
            .configuration()
            .latest_config_write(AppKind::Codex)
            .unwrap(),
        Some(history)
    );
    let target = state.target(AppKind::Codex).unwrap();
    assert_eq!(
        fs::read_to_string(target.with_file_name("auth.json")).unwrap(),
        "account-owned-by-codex"
    );
    assert!(!state
        .root()
        .join("configuration/switch-intent.json")
        .exists());
    gateway.shutdown();
}

#[test]
fn transaction_external_change_preserves_bytes_until_explicit_backup_recovery() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&state);
    crash_after_config_write(&state, &gateway);
    let target = state.target(AppKind::Codex).unwrap();
    fs::write(&target, "external = 'edit'\n").unwrap();
    assert!(transaction::recover(&state, &gateway)
        .unwrap_err()
        .contains("前后快照"));
    assert_eq!(fs::read_to_string(&target).unwrap(), "external = 'edit'\n");
    let pending = asb_switch::pending_config_write(&FsIo, &state.backup_dir(), AppKind::Codex)
        .unwrap()
        .unwrap();
    let outcome = transaction::restore_pending_backup(&state, &gateway, &pending.backup.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        fs::read_to_string(outcome.pre_restore_backup.backup_path).unwrap(),
        "external = 'edit'\n"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), BEFORE);
    assert!(state
        .configuration()
        .latest_config_write(AppKind::Codex)
        .unwrap()
        .is_none());
    assert_eq!(
        fs::read_to_string(target.with_file_name("auth.json")).unwrap(),
        "account-owned-by-codex"
    );
    transaction::recover(&state, &gateway).unwrap();
    gateway.shutdown();
}

#[test]
fn transaction_recovery_does_not_adopt_a_newer_profile() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&state);
    let record = crash_after_config_write(&state, &gateway);
    let target = state.target(AppKind::Codex).unwrap();
    let confirmed = fs::read_to_string(&target).unwrap();
    state
        .configuration()
        .update_provider(
            &record.profile.id,
            draft("external-model"),
            &record.file_hash,
        )
        .unwrap();
    assert!(transaction::recover(&state, &gateway)
        .unwrap_err()
        .contains("供应商版本已变化"));
    assert_eq!(fs::read_to_string(target).unwrap(), confirmed);
    assert!(state
        .root()
        .join("configuration/switch-intent.json")
        .exists());
    gateway.shutdown();
}

#[test]
fn pending_save_binds_the_confirmed_profile_revision_before_config_intent() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let record = state
        .configuration()
        .create_provider(draft("before-model"))
        .unwrap();
    let (app, original) = state
        .configuration()
        .load_provider_file(&record.profile.id)
        .unwrap();
    let candidate =
        asb_core::ProviderProfile::from_draft(record.profile.id.clone(), draft("confirmed-model"));
    profile_rollback::save(
        &state,
        app,
        &original,
        &candidate,
        "before-hash",
        "after-hash",
    )
    .unwrap();
    let saved = state
        .configuration()
        .update_provider(
            &record.profile.id,
            draft("confirmed-model"),
            &record.file_hash,
        )
        .unwrap();
    profile_rollback::validate_saved_revision(&state, &record.profile.id, &saved.file_hash)
        .unwrap();
    let external = state
        .configuration()
        .update_provider(
            &record.profile.id,
            draft("external-model"),
            &saved.file_hash,
        )
        .unwrap();
    assert!(profile_rollback::validate_saved_revision(
        &state,
        &record.profile.id,
        &external.file_hash
    )
    .is_err());
    assert!(profile_rollback::restore(&state, &record.profile.id, &saved.file_hash).is_err());
    assert_eq!(
        state
            .configuration()
            .find_provider_record(&record.profile.id)
            .unwrap()
            .file_hash,
        external.file_hash
    );
}

#[test]
fn pending_save_does_not_repreview_external_client_changes() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&state);
    let target = state.target(AppKind::Codex).unwrap();
    fs::write(&target, BEFORE).unwrap();
    let record = state
        .configuration()
        .create_provider(draft("before-model"))
        .unwrap();
    let (app, original) = state
        .configuration()
        .load_provider_file(&record.profile.id)
        .unwrap();
    let candidate =
        asb_core::ProviderProfile::from_draft(record.profile.id.clone(), draft("confirmed-model"));
    let projection = plan::build_plan_for_profile(&state, &gateway, candidate.clone()).unwrap();
    let confirmed = plan::preview_projection(&state, &projection).unwrap();
    profile_rollback::save(
        &state,
        app,
        &original,
        &candidate,
        &confirmed.content_hash,
        &confirmed.rendered_hash,
    )
    .unwrap();
    profile_rollback::validate_projection(&state, app, &confirmed).unwrap();
    fs::write(&target, "host_setting = 'new'\n").unwrap();
    let reinterpreted = plan::preview_projection(&state, &projection).unwrap();
    assert!(profile_rollback::validate_projection(&state, app, &reinterpreted).is_err());
    assert_eq!(
        fs::read_to_string(target).unwrap(),
        "host_setting = 'new'\n"
    );
    gateway.shutdown();
}
