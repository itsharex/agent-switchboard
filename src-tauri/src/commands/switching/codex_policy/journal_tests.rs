use super::*;
fn journal() -> Journal {
    let empty = GatewayActivationSnapshot {
        app: asb_core::AppKind::Codex,
        route: None,
    };
    Journal {
        version: 1,
        target: "isolated/config.toml".into(),
        before_policy: None,
        after_policy: "after-policy".into(),
        before_config_hash: "before-config".into(),
        after_config_hash: "after-config".into(),
        before_auth_hash: Some("before-auth".into()),
        after_auth_hash: Some("after-auth".into()),
        before_route: empty.clone(),
        after_route: empty,
    }
}
#[test]
fn crash_before_native_write_restores_old_policy_and_after_write_keeps_new_policy() {
    let journal = journal();
    assert!(recovery_side(
        &journal,
        "before-config",
        Some("before-auth"),
        Some("after-policy"),
        false
    )
    .unwrap());
    assert!(!recovery_side(
        &journal,
        "after-config",
        Some("after-auth"),
        Some("after-policy"),
        false
    )
    .unwrap());
}
#[test]
fn external_auth_config_or_policy_changes_are_never_overwritten() {
    let journal = journal();
    assert!(recovery_side(
        &journal,
        "external-config",
        Some("before-auth"),
        Some("after-policy"),
        false
    )
    .is_err());
    assert!(recovery_side(
        &journal,
        "before-config",
        Some("new-login"),
        Some("after-policy"),
        false
    )
    .is_err());
    assert!(recovery_side(
        &journal,
        "before-config",
        Some("before-auth"),
        Some("external-policy"),
        false
    )
    .is_err());
}
#[test]
fn policy_only_changes_recover_forward_after_confirmation_or_rollback_on_failure() {
    let mut journal = journal();
    journal.after_config_hash = journal.before_config_hash.clone();
    journal.after_auth_hash = journal.before_auth_hash.clone();
    assert!(!recovery_side(
        &journal,
        "before-config",
        Some("before-auth"),
        Some("after-policy"),
        false
    )
    .unwrap());
    assert!(recovery_side(
        &journal,
        "before-config",
        Some("before-auth"),
        Some("after-policy"),
        true
    )
    .unwrap());
}
#[test]
fn malformed_journal_is_preserved_and_missing_files_are_not_created_on_read() {
    let root = tempfile::tempdir().unwrap();
    assert!(load(root.path()).unwrap().is_none());
    assert!(!policy::pending_path(root.path()).exists());
    std::fs::create_dir_all(policy::pending_path(root.path()).parent().unwrap()).unwrap();
    std::fs::write(policy::pending_path(root.path()), "broken").unwrap();
    assert!(load(root.path()).is_err());
    assert_eq!(
        std::fs::read_to_string(policy::pending_path(root.path())).unwrap(),
        "broken"
    );
}
