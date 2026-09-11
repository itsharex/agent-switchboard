use super::{plan, profile_rollback, transaction};
use crate::{gateway::GatewayController, local_state::LocalState};
use asb_core::{
    contracts::{
        CodexCapabilities, CodexCatalogEntry, CodexEndpoint, CodexModelRoute, CodexProviderDraft,
        CodexProviderRecord, CodexUpstream,
    },
    AppKind,
};
use asb_switch::FsIo;
use std::{fs, panic::AssertUnwindSafe};

const BEFORE: &str = "model_provider = 'openai'\nmodel = 'before-model'\n";
const AUTH: &str = r#"{"auth_mode":"chatgpt","tokens":{"access_token":"at","refresh_token":"rt","id_token":"id"}}"#;

fn draft(revision: &str) -> CodexProviderDraft {
    CodexProviderDraft {
        name: format!("Codex fixture {revision}"),
        endpoint: CodexEndpoint("https://relay.example/v1".to_string()),
        api_key: "fixture-key".to_string(),
        upstream: CodexUpstream::Responses,
        request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
        default_model: revision.to_string(),
        catalog: vec![CodexCatalogEntry {
            id: revision.to_string(),
            context_window: 128_000,
            max_output_tokens: 16_384,
            function_tools: true,
            custom_tools: true,
            tool_search: true,
            reasoning: true,
            default_reasoning_level: asb_core::contracts::CodexReasoningLevel::High,
            supported_reasoning_levels: vec![
                asb_core::contracts::CodexReasoningLevel::None,
                asb_core::contracts::CodexReasoningLevel::High,
            ],
            images: true,
            compact: true,
        }],
        model_routes: vec![CodexModelRoute {
            client_model: revision.to_string(),
            upstream_model: format!("vendor-{revision}"),
        }],
        capabilities: CodexCapabilities {
            responses: true,
            compact: true,
            models: true,
            chat_completions: true,
            alpha_search: false,
            image_generation: false,
            image_edit: false,
            function_tools: true,
            custom_tools: true,
            tool_search: true,
            reasoning: true,
            chat_reasoning: asb_core::contracts::CodexChatReasoning::Unsupported,
        },
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Codex),
        notes: None,
        website_url: None,
        usage_query: None,
    }
}

fn crash_after_config_write(
    state: &LocalState,
    gateway: &GatewayController,
) -> CodexProviderRecord {
    let record = state
        .configuration()
        .create_codex_provider(draft("after-model"))
        .unwrap();
    let target = state.target(AppKind::Codex).unwrap();
    fs::write(&target, BEFORE).unwrap();
    fs::write(target.with_file_name("auth.json"), AUTH).unwrap();
    let projection = plan::build_plan(state, gateway, &record.profile.id).unwrap();
    let preview = plan::preview_projection(state, &projection).unwrap();
    assert_ne!(
        preview.content_hash, preview.rendered_hash,
        "third-party Codex projection replaces the selected model"
    );
    transaction::begin(
        state,
        gateway,
        AppKind::Codex,
        Some(&record.profile.id),
        &preview.rendered_hash,
        true,
        None,
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
        AUTH
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
        AUTH
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
        .update_codex_provider(
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
        .create_codex_provider(draft("before-model"))
        .unwrap();
    let original = state
        .configuration()
        .find_codex_provider_file(&record.profile.id)
        .unwrap();
    let candidate =
        draft("confirmed-model").into_file(record.profile.id.clone(), original.position);
    profile_rollback::save_codex(&state, &original, &candidate, "before-hash", "after-hash")
        .unwrap();
    let saved = state
        .configuration()
        .update_codex_provider(
            &record.profile.id,
            draft("confirmed-model"),
            &record.file_hash,
        )
        .unwrap();
    profile_rollback::validate_saved_revision(&state, &record.profile.id, &saved.file_hash)
        .unwrap();
    let external = state
        .configuration()
        .update_codex_provider(
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
            .list_codex_providers()
            .unwrap()
            .into_iter()
            .find(|item| item.profile.id == record.profile.id)
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
        .create_codex_provider(draft("before-model"))
        .unwrap();
    fs::write(target.with_file_name("auth.json"), AUTH).unwrap();
    let original = state
        .configuration()
        .find_codex_provider_file(&record.profile.id)
        .unwrap();
    let candidate =
        draft("confirmed-model").into_file(record.profile.id.clone(), original.position);
    let projection = plan::build_codex_plan(&state, &gateway, candidate.clone()).unwrap();
    let confirmed = plan::preview_projection(&state, &projection).unwrap();
    profile_rollback::save_codex(
        &state,
        &original,
        &candidate,
        &confirmed.content_hash,
        &confirmed.rendered_hash,
    )
    .unwrap();
    profile_rollback::validate_projection(&state, AppKind::Codex, &confirmed).unwrap();
    fs::write(&target, "host_setting = 'new'\n").unwrap();
    let reinterpreted = plan::preview_projection(&state, &projection).unwrap();
    assert!(profile_rollback::validate_projection(&state, AppKind::Codex, &reinterpreted).is_err());
    assert_eq!(
        fs::read_to_string(target).unwrap(),
        "host_setting = 'new'\n"
    );
    gateway.shutdown();
}
