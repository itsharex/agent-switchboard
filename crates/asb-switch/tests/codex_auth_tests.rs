//! Provider projection cannot read, write, back up, or recover the account cache.
mod common;
use asb_core::{AppKind, RouteMode, SwitchPlan};
use asb_switch::io::FsIo;
use asb_switch::{execute, read_preview, restore, SwitchRequest};
use common::{codex_plan, setup, write};
use std::fs;

const AUTH: &str = r#"{"auth_mode":"chatgpt","tokens":{"access_token":"OFFICIAL_TOKEN"}}"#;

fn official() -> SwitchPlan {
    let mut plan = codex_plan("official", "https://unused.example", "gpt-5.4", "unused");
    plan.profile.route_mode = RouteMode::Official;
    plan.profile.base_url = None;
    plan.profile.api_key.clear();
    plan.profile.upstream_protocol = None;
    plan.profile.responses_options = None;
    plan.profile.model = None;
    SwitchPlan::direct(plan.profile, plan.client_settings)
}

#[test]
fn switching_and_restoring_only_backup_and_write_configuration() {
    let (_dir, target, backups) = setup(AppKind::Codex, "model_provider = \"openai\"\n");
    let auth = target.parent().unwrap().join("auth.json");
    write(&auth, AUTH);
    let custom = codex_plan(
        "relay",
        "https://thirdparty.example",
        "gpt-5.4",
        "THIRD_PARTY_KEY",
    );
    for plan in [custom, official()] {
        let preview = read_preview(&FsIo, &target, &plan, &backups.to_string_lossy()).unwrap();
        assert!(!preview.content.contains("THIRD_PARTY_KEY"));
        assert!(preview
            .preview
            .changes
            .iter()
            .all(|change| !change.key.starts_with("auth.")));
        let outcome = execute(
            &FsIo,
            &SwitchRequest {
                target: &target,
                plan: &plan,
                backup_dir: &backups,
                expected_hash: &preview.content_hash,
                expected_rendered_hash: &preview.rendered_hash,
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(outcome.changed, vec![target.to_string_lossy()]);
        assert_eq!(fs::read_to_string(&auth).unwrap(), AUTH);
        assert!(asb_switch::list_backups(&FsIo, &backups)
            .iter()
            .all(|b| b.linked_backup_id.is_none() && b.target_path != auth.to_string_lossy()));
        restore(&FsIo, &outcome.backup, &target, |_| Ok(())).unwrap();
        assert_eq!(fs::read_to_string(&auth).unwrap(), AUTH);
    }
}

#[test]
fn oauth_refresh_after_preview_is_not_a_configuration_conflict() {
    let (_dir, target, backups) = setup(AppKind::Codex, "model_provider = \"openai\"\n");
    let auth = target.parent().unwrap().join("auth.json");
    write(&auth, AUTH);
    let plan = codex_plan(
        "relay",
        "https://thirdparty.example",
        "gpt-5.4",
        "THIRD_PARTY_KEY",
    );
    let preview = read_preview(&FsIo, &target, &plan, &backups.to_string_lossy()).unwrap();
    write(&auth, "NEW_OFFICIAL_LOGIN");
    execute(
        &FsIo,
        &SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backups,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(fs::read_to_string(&auth).unwrap(), "NEW_OFFICIAL_LOGIN");
}

#[test]
fn failed_state_commit_preserves_concurrent_logout() {
    let (_dir, target, backups) = setup(AppKind::Codex, "model_provider = \"openai\"\n");
    let original = fs::read_to_string(&target).unwrap();
    let auth = target.parent().unwrap().join("auth.json");
    write(&auth, AUTH);
    let plan = codex_plan("relay", "https://thirdparty.example", "gpt-5.4", "key");
    let preview = read_preview(&FsIo, &target, &plan, &backups.to_string_lossy()).unwrap();
    let error = execute(
        &FsIo,
        &SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backups,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        |_| {
            fs::remove_file(&auth).unwrap();
            Err("state failure".into())
        },
    )
    .unwrap_err();
    assert!(matches!(
        error,
        asb_switch::SwitchError::CommitFailed {
            recovery: asb_switch::RecoveryOutcome::Restored { .. },
            ..
        }
    ));
    assert_eq!(fs::read_to_string(&target).unwrap(), original);
    assert!(!auth.exists());
}

#[test]
fn official_switch_does_not_create_missing_or_repair_broken_auth() {
    let (_dir, target, backups) = setup(AppKind::Codex, "");
    let auth = target.parent().unwrap().join("auth.json");
    for value in [None, Some("broken")] {
        if let Some(value) = value {
            write(&auth, value);
        }
        let plan = official();
        let preview = read_preview(&FsIo, &target, &plan, &backups.to_string_lossy()).unwrap();
        execute(
            &FsIo,
            &SwitchRequest {
                target: &target,
                plan: &plan,
                backup_dir: &backups,
                expected_hash: &preview.content_hash,
                expected_rendered_hash: &preview.rendered_hash,
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&auth).ok().as_deref(), value);
    }
}

#[test]
fn retired_projection_is_converted_inside_preview_and_cannot_be_restored() {
    let original = "model_provider = \"agent_switchboard\"\n[model_providers.agent_switchboard]\nbase_url = \"https://old.example\"\nrequest_timeout_ms = 999\n";
    let (_dir, target, backups) = setup(AppKind::Codex, original);
    let plan = official();
    let preview = read_preview(&FsIo, &target, &plan, &backups.to_string_lossy()).unwrap();
    assert_eq!(preview.content_hash, asb_switch::sha256_hex(original));
    assert!(preview
        .preview
        .changes
        .iter()
        .any(|change| change.key == "model_providers.agent_switchboard.base_url"));
    assert!(preview.content.contains("request_timeout_ms = 999"));
    let outcome = execute(
        &FsIo,
        &SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backups,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        |_| Ok(()),
    )
    .unwrap();
    let current = fs::read_to_string(&target).unwrap();
    assert!(current.contains("model_provider = \"openai\""));
    assert_eq!(
        fs::read_to_string(&outcome.backup.backup_path).unwrap(),
        original
    );
    assert!(restore(&FsIo, &outcome.backup, &target, |_| Ok(())).is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), current);
}
