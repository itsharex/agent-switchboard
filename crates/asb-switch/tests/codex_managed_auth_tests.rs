mod common;
use asb_core::{contracts::CodexManagedAuth, AppKind, RouteMode, SwitchPlan};
use asb_switch::{execute_codex, io::FsIo, read_preview, SwitchRequest};
use common::{codex_plan, setup, write};
use std::fs;
fn plan() -> SwitchPlan {
    let mut plan = codex_plan("official", "https://unused.example", "unused", "unused");
    plan.profile.route_mode = RouteMode::Official;
    plan.profile.base_url = None; plan.profile.api_key.clear();
    plan.profile.upstream_protocol = None; plan.profile.responses_options = None; plan.profile.model = None;
    SwitchPlan::direct(plan.profile, plan.client_settings).with_codex_managed_auth(CodexManagedAuth {
        managed_id: "account-one".into(), account_id: "workspace-one".into(), subject: "user-one".into(),
        id_token: "MANAGED_ID_TOKEN".into(), access_token: "MANAGED_ACCESS".into(), refresh_token: "MANAGED_REFRESH".into(),
        generation: 2, last_refresh: "2026-09-13T00:00:00Z".into(),
    })
}
#[test]
fn managed_account_projection_is_paired_backed_up_and_secret_free() {
    let (_directory, target, backups) = setup(AppKind::Codex, "model_provider = \"openai\"\n# keep\n");
    let auth = target.with_file_name("auth.json"); write(&auth, r#"{"host_note":"keep","tokens":{"access_token":"old"}}"#);
    let plan = plan(); let before = fs::read_to_string(&auth).unwrap();
    let preview = read_preview(&FsIo, &target, &plan, &backups.to_string_lossy()).unwrap();
    assert_eq!(fs::read_to_string(&auth).unwrap(), before);
    assert!(!serde_json::to_string(&preview).unwrap().contains("MANAGED_"));
    assert!(!format!("{plan:?}").contains("MANAGED_"));
    let outcome = execute_codex(&FsIo, &SwitchRequest { target: &target, plan: &plan, backup_dir: &backups,
        expected_hash: &preview.content_hash, expected_rendered_hash: &preview.rendered_hash },
        preview.auth_hash.as_deref(), preview.auth_existed, preview.auth_rendered_hash.as_deref(), |_| Ok(())).unwrap();
    let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&auth).unwrap()).unwrap();
    assert_eq!(value["host_note"], "keep"); assert_eq!(value["auth_mode"], "chatgpt");
    assert_eq!(value["tokens"]["access_token"], "MANAGED_ACCESS");
    assert_eq!(value["asb_managed_account"]["generation"], 2);
    assert_eq!(outcome.changed.len(), 2);
    assert!(asb_switch::list_backups(&FsIo, &backups).iter().any(|backup|
        backup.linked_backup_id.as_ref() == Some(&outcome.backup.id) && fs::read_to_string(&backup.backup_path).unwrap() == before));
}
#[test]
fn managed_switch_refuses_native_login_changes_after_preview() {
    let (_directory, target, backups) = setup(AppKind::Codex, ""); let auth = target.with_file_name("auth.json");
    write(&auth, "{}"); let plan = plan();
    let preview = read_preview(&FsIo, &target, &plan, &backups.to_string_lossy()).unwrap();
    write(&auth, r#"{"host_note":"external-login"}"#);
    let result = execute_codex(&FsIo, &SwitchRequest { target: &target, plan: &plan, backup_dir: &backups,
        expected_hash: &preview.content_hash, expected_rendered_hash: &preview.rendered_hash },
        preview.auth_hash.as_deref(), preview.auth_existed, preview.auth_rendered_hash.as_deref(), |_| Ok(()));
    assert!(result.is_err()); assert!(fs::read_to_string(&auth).unwrap().contains("external-login"));
    assert_eq!(fs::read_to_string(&target).unwrap(), "");
}
#[test]
fn failed_managed_commit_restores_both_files_and_missing_auth_remains_missing() {
    for existed in [true, false] {
        let (_directory, target, backups) = setup(AppKind::Codex, "# before\n"); let auth = target.with_file_name("auth.json");
        if existed { write(&auth, "{}"); }
        let plan = plan(); let preview = read_preview(&FsIo, &target, &plan, &backups.to_string_lossy()).unwrap();
        let result = execute_codex(&FsIo, &SwitchRequest { target: &target, plan: &plan, backup_dir: &backups,
            expected_hash: &preview.content_hash, expected_rendered_hash: &preview.rendered_hash },
            preview.auth_hash.as_deref(), preview.auth_existed, preview.auth_rendered_hash.as_deref(), |_| Err("fixture-failure".into()));
        assert!(result.is_err()); assert_eq!(fs::read_to_string(&target).unwrap(), "# before\n");
        assert_eq!(auth.exists(), existed); if existed { assert_eq!(fs::read_to_string(&auth).unwrap(), "{}"); }
    }
}

#[test]
fn restore_and_undo_restore_keep_auth_backups_linked_and_restore_both_files() {
    let (_directory, target, backups) = setup(AppKind::Codex, "# before\n");
    let auth = target.with_file_name("auth.json"); write(&auth, "{}");
    let plan = plan(); let preview = read_preview(&FsIo, &target, &plan, &backups.to_string_lossy()).unwrap();
    let switched = execute_codex(&FsIo, &SwitchRequest { target: &target, plan: &plan, backup_dir: &backups,
        expected_hash: &preview.content_hash, expected_rendered_hash: &preview.rendered_hash },
        preview.auth_hash.as_deref(), preview.auth_existed, preview.auth_rendered_hash.as_deref(), |_| Ok(())).unwrap();
    let activated_auth = fs::read_to_string(&auth).unwrap();
    let restored = asb_switch::restore(&FsIo, &switched.backup, &target, |_| Ok(())).unwrap();
    assert_eq!(fs::read_to_string(&auth).unwrap(), "{}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "# before\n");
    let pair = asb_switch::list_backups(&FsIo, &backups).into_iter()
        .find(|b| b.linked_backup_id.as_ref() == Some(&restored.pre_restore_backup.id)).unwrap();
    assert_eq!(pair.reason, "codex-auth-projection");
    asb_switch::restore(&FsIo, &restored.pre_restore_backup, &target, |_| Ok(())).unwrap();
    assert_eq!(fs::read_to_string(&auth).unwrap(), activated_auth);
}
#[test]
fn restore_rejects_a_historical_generation_before_touching_live_files() {
    let (_directory, target, backups) = setup(AppKind::Codex, "# before\n");
    let auth = target.with_file_name("auth.json");
    let before = r#"{"auth_mode":"chatgpt","asb_managed_account":{"id":"account-one","generation":1},"tokens":{"access_token":"old"}}"#;
    write(&auth, before); let plan = plan();
    let preview = read_preview(&FsIo, &target, &plan, &backups.to_string_lossy()).unwrap();
    let switched = execute_codex(&FsIo, &SwitchRequest { target: &target, plan: &plan, backup_dir: &backups,
        expected_hash: &preview.content_hash, expected_rendered_hash: &preview.rendered_hash },
        preview.auth_hash.as_deref(), preview.auth_existed, preview.auth_rendered_hash.as_deref(), |_| Ok(())).unwrap();
    let current_auth = fs::read_to_string(&auth).unwrap(); let current_config = fs::read_to_string(&target).unwrap();
    assert!(asb_switch::restore(&FsIo, &switched.backup, &target, |_| Ok(())).is_err());
    assert_eq!(fs::read_to_string(&auth).unwrap(), current_auth);
    assert_eq!(fs::read_to_string(&target).unwrap(), current_config);
}
#[test]
fn failed_restore_never_overwrites_a_new_native_login_from_the_commit_callback() {
    let (_directory, target, backups) = setup(AppKind::Codex, "# before\n");
    let auth = target.with_file_name("auth.json"); write(&auth, "{}"); let plan = plan();
    let preview = read_preview(&FsIo, &target, &plan, &backups.to_string_lossy()).unwrap();
    let switched = execute_codex(&FsIo, &SwitchRequest { target: &target, plan: &plan, backup_dir: &backups,
        expected_hash: &preview.content_hash, expected_rendered_hash: &preview.rendered_hash },
        preview.auth_hash.as_deref(), preview.auth_existed, preview.auth_rendered_hash.as_deref(), |_| Ok(())).unwrap();
    let current_config = fs::read_to_string(&target).unwrap();
    assert!(asb_switch::restore(&FsIo, &switched.backup, &target, |_| {
        write(&auth, "external native login"); Err("commit failed".into())
    }).is_err());
    assert_eq!(fs::read_to_string(&auth).unwrap(), "external native login");
    assert_eq!(fs::read_to_string(&target).unwrap(), current_config);
    assert!(asb_switch::pending_config_write(&FsIo, &backups, AppKind::Codex).unwrap().is_some());
}
#[test]
fn no_oauth_gateway_login_and_explicit_removal_use_paired_auth_transactions() {
    for preserve in [false, true] {
        let (_directory, target, backups) = setup(AppKind::Codex, ""); let auth = target.with_file_name("auth.json");
        write(&auth, r#"{"auth_mode":"chatgpt","tokens":{"id_token":"id","access_token":"access","refresh_token":"refresh"},"unowned":"keep"}"#);
        let plan = codex_plan("relay", "https://relay.example", "model", "UPSTREAM_SECRET").with_codex_preserve_official_login(preserve);
        let preview = read_preview(&FsIo, &target, &plan, &backups.to_string_lossy()).unwrap();
        execute_codex(&FsIo, &SwitchRequest { target: &target, plan: &plan, backup_dir: &backups,
            expected_hash: &preview.content_hash, expected_rendered_hash: &preview.rendered_hash },
            preview.auth_hash.as_deref(), preview.auth_existed, preview.auth_rendered_hash.as_deref(), |_| Ok(())).unwrap();
        let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&auth).unwrap()).unwrap();
        assert_eq!(value.get("tokens").is_some(), preserve); assert_eq!(value["unowned"], "keep");
        assert!(!fs::read_to_string(&auth).unwrap().contains("UPSTREAM_SECRET"));
    }
}

#[test]
fn auth_hashes_are_mandatory_when_the_confirmed_operation_changes_auth() {
    let (_directory, target, backups) = setup(AppKind::Codex, "# before\n");
    let auth = target.with_file_name("auth.json"); write(&auth, "{}"); let plan = plan();
    let preview = read_preview(&FsIo, &target, &plan, &backups.to_string_lossy()).unwrap();
    let error = execute_codex(&FsIo, &SwitchRequest { target: &target, plan: &plan, backup_dir: &backups,
        expected_hash: &preview.content_hash, expected_rendered_hash: &preview.rendered_hash }, None, None, None, |_| Ok(())).unwrap_err();
    assert!(matches!(error, asb_switch::SwitchError::PlanChanged));
    assert_eq!(fs::read_to_string(&auth).unwrap(), "{}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "# before\n");
}
