//! Codex dual-file switching: `config.toml` plus the linked `auth.json`
//! credential cache, and the restore of their paired snapshots.

mod common;

use asb_core::contracts::AppKind;
use asb_switch::io::FsIo;
use asb_switch::{RecoveryOutcome, SwitchError};
use common::{codex_plan, lock_path_exists, setup, write, FailingIo};
use std::fs;

#[test]
fn codex_builtin_openai_switches_relays_without_changing_the_session_provider() {
    let initial = r#"threads = 8
model = "gpt-5.1"
model_provider = "openai"
openai_base_url = "https://legacy.internal/v1"
experimental_bearer_token = "LEGACY_TOKEN"

[model_providers.OpenAi]
host_extension = "preserve"

[model_providers.gateway]
name = "Gateway"
base_url = "https://gateway.internal/v1"
wire_api = "responses"
"#;
    let (_dir, target, backup_dir) = setup(AppKind::Codex, initial);
    let auth_path = target.parent().expect("target parent").join("auth.json");
    let auth = "{\"auth_mode\":\"chatgpt\",\"OPENAI_API_KEY\":null,\"tokens\":{\"access_token\":\"OFFICIAL_TOKEN\"}}";
    write(&auth_path, auth);
    let io = FsIo;

    let custom = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );
    let custom_preview = asb_switch::read_codex_preview(
        &io,
        &target,
        &auth_path,
        &custom,
        &backup_dir.to_string_lossy(),
    )
    .expect("custom preview");
    assert!(!custom_preview.content.contains("CODEX_RELAY_B_KEY"));
    assert!(custom_preview
        .preview
        .changes
        .iter()
        .any(|change| change.key == "auth.json.OPENAI_API_KEY"
            && change.after.as_deref() == Some("••••••••")));
    let outcome = asb_switch::execute_codex(
        &io,
        &asb_switch::CodexSwitchRequest {
            target: &target,
            auth_target: &auth_path,
            plan: &custom,
            backup_dir: &backup_dir,
            expected_hash: &custom_preview.content_hash,
            expected_rendered_hash: &custom_preview.rendered_hash,
        },
        |_| Ok(()),
    )
    .expect("custom switch");

    let custom_text = fs::read_to_string(&target).expect("custom config");
    assert!(custom_text.contains("model_provider = \"openai\""));
    assert!(custom_text.contains("openai_base_url = \"https://relay-b.internal/v1\""));
    assert!(!custom_text.contains("experimental_bearer_token"));
    assert!(custom_text.contains("host_extension = \"preserve\""));
    assert!(custom_text.contains("[model_providers.gateway]"));
    let active_auth: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&auth_path).expect("active auth"))
            .expect("valid active auth");
    assert_eq!(active_auth["auth_mode"], "apikey");
    assert_eq!(active_auth["OPENAI_API_KEY"], "CODEX_RELAY_B_KEY");
    assert_eq!(active_auth["tokens"]["access_token"], "OFFICIAL_TOKEN");
    assert!(outcome
        .changed
        .contains(&auth_path.to_string_lossy().to_string()));

    let mut official = custom;
    official.profile.route_mode = asb_core::RouteMode::Official;
    official.profile.model = None;
    official.profile.base_url = None;
    official.profile.api_key.clear();
    official.profile.upstream_protocol = None;
    official.profile.max_output_tokens = None.into();
    official.profile.model_options = None;
    let official_preview = asb_switch::read_codex_preview(
        &io,
        &target,
        &auth_path,
        &official,
        &backup_dir.to_string_lossy(),
    )
    .expect("official preview");
    asb_switch::execute_codex(
        &io,
        &asb_switch::CodexSwitchRequest {
            target: &target,
            auth_target: &auth_path,
            plan: &official,
            backup_dir: &backup_dir,
            expected_hash: &official_preview.content_hash,
            expected_rendered_hash: &official_preview.rendered_hash,
        },
        |_| Ok(()),
    )
    .expect("official switch");

    let official_text = fs::read_to_string(&target).expect("official config");
    assert!(official_text.contains("model_provider = \"openai\""));
    assert!(!official_text.contains("openai_base_url"));
    assert!(!official_text.contains("experimental_bearer_token"));
    assert!(official_text.contains("host_extension = \"preserve\""));
    assert!(official_text.contains("[model_providers.gateway]"));
    let official_auth: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&auth_path).expect("official auth"))
            .expect("valid official auth");
    assert_eq!(official_auth["auth_mode"], "chatgpt");
    assert!(official_auth["OPENAI_API_KEY"].is_null());
    assert_eq!(official_auth["tokens"]["access_token"], "OFFICIAL_TOKEN");
}

#[test]
fn codex_restore_reconciles_the_linked_auth_snapshot() {
    let initial = r#"model = "gpt-5.1"
model_provider = "openai"
openai_base_url = "https://relay-a.internal/v1"
"#;
    let (_dir, target, backup_dir) = setup(AppKind::Codex, initial);
    let auth_path = target.parent().expect("target parent").join("auth.json");
    let auth = r#"{"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{"access_token":"OFFICIAL_TOKEN"}}"#;
    write(&auth_path, auth);
    let plan = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );
    let io = FsIo;
    let preview = asb_switch::read_codex_preview(
        &io,
        &target,
        &auth_path,
        &plan,
        &backup_dir.to_string_lossy(),
    )
    .expect("preview");
    let switched = asb_switch::execute_codex(
        &io,
        &asb_switch::CodexSwitchRequest {
            target: &target,
            auth_target: &auth_path,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        |_| Ok(()),
    )
    .expect("switch");
    let auth_backup = asb_switch::list_backups(&io, &backup_dir)
        .into_iter()
        .find(|backup| backup.linked_backup_id.as_deref() == Some(switched.backup.id.as_str()))
        .expect("linked auth backup");

    let restored = asb_switch::restore_codex(
        &io,
        &switched.backup,
        &auth_backup,
        &target,
        &auth_path,
        |_| Ok(()),
    )
    .expect("restore linked pair");

    assert_eq!(fs::read_to_string(&target).unwrap(), initial);
    assert_eq!(fs::read_to_string(&auth_path).unwrap(), auth);
    assert!(asb_switch::list_backups(&io, &backup_dir)
        .iter()
        .any(|backup| backup.linked_backup_id.as_deref()
            == Some(restored.pre_restore_backup.id.as_str())));
}

#[test]
fn codex_restore_keeps_an_unchanged_auth_snapshot_paired_with_its_config() {
    let initial = r#"model = "gpt-5.1"
model_provider = "openai"
openai_base_url = "https://relay-a.internal/v1"
"#;
    let (_dir, target, backup_dir) = setup(AppKind::Codex, initial);
    let auth_path = target.parent().expect("target parent").join("auth.json");
    let relay_a_auth = r#"{"auth_mode":"apikey","OPENAI_API_KEY":"CODEX_RELAY_A_KEY"}"#;
    write(&auth_path, relay_a_auth);
    let io = FsIo;

    let relay_a = codex_plan(
        "Relay A",
        "https://relay-a.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_A_KEY",
    );
    let relay_a_preview = asb_switch::read_codex_preview(
        &io,
        &target,
        &auth_path,
        &relay_a,
        &backup_dir.to_string_lossy(),
    )
    .expect("relay A preview");
    let relay_a_switch = asb_switch::execute_codex(
        &io,
        &asb_switch::CodexSwitchRequest {
            target: &target,
            auth_target: &auth_path,
            plan: &relay_a,
            backup_dir: &backup_dir,
            expected_hash: &relay_a_preview.content_hash,
            expected_rendered_hash: &relay_a_preview.rendered_hash,
        },
        |_| Ok(()),
    )
    .expect("relay A config-only switch");
    assert!(!relay_a_switch
        .changed
        .contains(&auth_path.to_string_lossy().to_string()));
    let relay_a_auth_backup = asb_switch::list_backups(&io, &backup_dir)
        .into_iter()
        .find(|backup| {
            backup.linked_backup_id.as_deref() == Some(relay_a_switch.backup.id.as_str())
        })
        .expect("unchanged auth snapshot is still linked");

    let relay_b = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.3",
        "CODEX_RELAY_B_KEY",
    );
    let relay_b_preview = asb_switch::read_codex_preview(
        &io,
        &target,
        &auth_path,
        &relay_b,
        &backup_dir.to_string_lossy(),
    )
    .expect("relay B preview");
    asb_switch::execute_codex(
        &io,
        &asb_switch::CodexSwitchRequest {
            target: &target,
            auth_target: &auth_path,
            plan: &relay_b,
            backup_dir: &backup_dir,
            expected_hash: &relay_b_preview.content_hash,
            expected_rendered_hash: &relay_b_preview.rendered_hash,
        },
        |_| Ok(()),
    )
    .expect("relay B switch");

    asb_switch::restore_codex(
        &io,
        &relay_a_switch.backup,
        &relay_a_auth_backup,
        &target,
        &auth_path,
        |_| Ok(()),
    )
    .expect("restore paired relay A backup");

    assert_eq!(fs::read_to_string(&target).unwrap(), initial);
    assert_eq!(fs::read_to_string(&auth_path).unwrap(), relay_a_auth);
}

#[test]
fn codex_restore_removes_an_auth_cache_that_did_not_exist_before_switching() {
    let initial = r#"model = "gpt-5.1"
model_provider = "openai"
openai_base_url = "https://relay-a.internal/v1"
"#;
    let (_dir, target, backup_dir) = setup(AppKind::Codex, initial);
    let auth_path = target.parent().expect("target parent").join("auth.json");
    let plan = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );
    let io = FsIo;
    let preview = asb_switch::read_codex_preview(
        &io,
        &target,
        &auth_path,
        &plan,
        &backup_dir.to_string_lossy(),
    )
    .expect("preview");
    let switched = asb_switch::execute_codex(
        &io,
        &asb_switch::CodexSwitchRequest {
            target: &target,
            auth_target: &auth_path,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        |_| Ok(()),
    )
    .expect("switch");
    let auth_backup = asb_switch::list_backups(&io, &backup_dir)
        .into_iter()
        .find(|backup| backup.linked_backup_id.as_deref() == Some(switched.backup.id.as_str()))
        .expect("linked auth backup");

    asb_switch::restore_codex(
        &io,
        &switched.backup,
        &auth_backup,
        &target,
        &auth_path,
        |_| Ok(()),
    )
    .expect("restore linked pair");

    assert_eq!(fs::read_to_string(&target).unwrap(), initial);
    assert!(!auth_path.exists());
}

#[test]
fn codex_official_switch_does_not_create_a_missing_auth_cache() {
    let initial = r#"model = "gpt-5.1"
model_provider = "openai"
"#;
    let (_dir, target, backup_dir) = setup(AppKind::Codex, initial);
    let auth_path = target.parent().expect("target parent").join("auth.json");
    let mut official = codex_plan(
        "OpenAI",
        "https://unused.example/v1",
        "unused-model",
        "unused-key",
    );
    official.profile.route_mode = asb_core::RouteMode::Official;
    official.profile.model = None;
    official.profile.base_url = None;
    official.profile.api_key.clear();
    official.profile.upstream_protocol = None;
    official.profile.max_output_tokens = None.into();
    official.profile.model_options = None;
    let io = FsIo;
    let preview = asb_switch::read_codex_preview(
        &io,
        &target,
        &auth_path,
        &official,
        &backup_dir.to_string_lossy(),
    )
    .expect("preview");
    assert!(preview
        .preview
        .changes
        .iter()
        .all(|change| !change.key.starts_with("auth.json.")));
    asb_switch::execute_codex(
        &io,
        &asb_switch::CodexSwitchRequest {
            target: &target,
            auth_target: &auth_path,
            plan: &official,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        |_| Ok(()),
    )
    .expect("official switch");

    assert!(!auth_path.exists());
}

#[test]
fn codex_auth_write_failure_rolls_back_the_configuration_too() {
    for stage in ["auth-temp-write", "auth-atomic-replace"] {
        let initial = r#"model = "gpt-5.1"
model_provider = "openai"
openai_base_url = "https://relay-a.internal/v1"
"#;
        let (_dir, target, backup_dir) = setup(AppKind::Codex, initial);
        let auth_path = target.parent().expect("target parent").join("auth.json");
        let auth = r#"{"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{"access_token":"OFFICIAL_TOKEN"}}"#;
        write(&auth_path, auth);
        let plan = codex_plan(
            "Relay B",
            "https://relay-b.internal/v1",
            "gpt-5.2",
            "CODEX_RELAY_B_KEY",
        );
        let io = FailingIo::new();
        io.fail_stage.set(Some(stage));
        let preview = asb_switch::read_codex_preview(
            &io,
            &target,
            &auth_path,
            &plan,
            &backup_dir.to_string_lossy(),
        )
        .expect("preview");

        let error = asb_switch::execute_codex(
            &io,
            &asb_switch::CodexSwitchRequest {
                target: &target,
                auth_target: &auth_path,
                plan: &plan,
                backup_dir: &backup_dir,
                expected_hash: &preview.content_hash,
                expected_rendered_hash: &preview.rendered_hash,
            },
            |_| Ok(()),
        )
        .expect_err(stage);

        assert!(matches!(error, SwitchError::CommitFailed { .. }));
        assert_eq!(fs::read_to_string(&target).unwrap(), initial, "{stage}");
        assert_eq!(fs::read_to_string(&auth_path).unwrap(), auth, "{stage}");
        assert!(!lock_path_exists(&target), "{stage}");
    }
}

#[test]
fn codex_state_commit_failure_rolls_back_both_files() {
    let initial = r#"model = "gpt-5.1"
model_provider = "openai"
openai_base_url = "https://relay-a.internal/v1"
"#;
    let (_dir, target, backup_dir) = setup(AppKind::Codex, initial);
    let auth_path = target.parent().expect("target parent").join("auth.json");
    let auth = r#"{"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{"access_token":"OFFICIAL_TOKEN"}}"#;
    write(&auth_path, auth);
    let plan = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );
    let io = FsIo;
    let preview = asb_switch::read_codex_preview(
        &io,
        &target,
        &auth_path,
        &plan,
        &backup_dir.to_string_lossy(),
    )
    .expect("preview");

    let error = asb_switch::execute_codex(
        &io,
        &asb_switch::CodexSwitchRequest {
            target: &target,
            auth_target: &auth_path,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        |_| Err("injected state failure".to_string()),
    )
    .expect_err("state failure");

    assert!(matches!(
        error,
        SwitchError::CommitFailed {
            stage: "state-save",
            recovery: RecoveryOutcome::Restored { .. },
            ..
        }
    ));
    assert_eq!(fs::read_to_string(&target).unwrap(), initial);
    assert_eq!(fs::read_to_string(&auth_path).unwrap(), auth);
}
