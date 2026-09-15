use super::*;
use asb_core::contracts::{ChangeKind, KeyChange, RouteMode};
use asb_switch::sha256_hex;
use serde_json::{json, Value};
use std::path::PathBuf;

struct Fixture {
    _paths: crate::test_client_paths::ClientPathGuard,
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let paths = crate::test_client_paths::redirect_client_paths();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("state");
        std::fs::create_dir_all(&root).unwrap();
        Self {
            _paths: paths,
            _dir: dir,
            root,
        }
    }
    fn document(&self, flag: ClaudeIntegrationFlag) -> Value {
        serde_json::from_str(&std::fs::read_to_string(flag.target().unwrap()).unwrap()).unwrap()
    }
    fn backups(&self) -> usize {
        std::fs::read_dir(self.root.join(BACKUP_DIR))
            .map(|entries| {
                entries
                    .filter(|entry| {
                        entry
                            .as_ref()
                            .unwrap()
                            .path()
                            .to_string_lossy()
                            .ends_with(".bak")
                    })
                    .count()
            })
            .unwrap_or(0)
    }
}

#[test]
fn plugin_marker_is_created_previewed_and_cleared_with_sibling_keys_kept() {
    let fixture = Fixture::new();
    let initial = view(&fixture.root).unwrap();
    assert_eq!(initial.policy, ClaudeIntegrationPolicy::default());
    assert!(initial
        .flags
        .iter()
        .all(|flag| !flag.exists && !flag.applied));

    let (plan, _) = preview(ClaudeIntegrationFlag::Plugin, true).unwrap();
    assert!(!plan.target_existed);
    assert_eq!(plan.content_hash, sha256_hex("{}"));
    assert_eq!(
        plan.changes,
        vec![KeyChange {
            key: "primaryApiKey".into(),
            kind: ChangeKind::Set,
            before: None,
            after: Some("\"any\"".into()),
        }]
    );
    let applied = apply(&fixture.root, &plan).unwrap();
    assert!(applied.flags[0].applied && applied.flags[0].exists);
    assert_eq!(
        fixture.document(ClaudeIntegrationFlag::Plugin),
        json!({"primaryApiKey": "any"})
    );
    assert_eq!(fixture.backups(), 1);
    assert!(matches!(
        apply(&fixture.root, &plan),
        Err(message) if message.contains("重新预览")
    ));

    std::fs::write(
        ClaudeIntegrationFlag::Plugin.target().unwrap(),
        json!({"primaryApiKey": "any", "theme": "dark"}).to_string(),
    )
    .unwrap();
    let (same, _) = preview(ClaudeIntegrationFlag::Plugin, true).unwrap();
    assert!(same.changes.is_empty());
    let untouched = apply(&fixture.root, &same).unwrap();
    assert!(untouched.flags[0].applied);
    assert_eq!(fixture.backups(), 1);

    let (clear, _) = preview(ClaudeIntegrationFlag::Plugin, false).unwrap();
    assert_eq!(clear.changes[0].kind, ChangeKind::Remove);
    apply(&fixture.root, &clear).unwrap();
    assert_eq!(
        fixture.document(ClaudeIntegrationFlag::Plugin),
        json!({"theme": "dark"})
    );
    assert_eq!(fixture.backups(), 2);
}

#[test]
fn onboarding_marker_keeps_the_rest_of_the_user_document() {
    let fixture = Fixture::new();
    let target = ClaudeIntegrationFlag::Onboarding.target().unwrap();
    std::fs::write(
        &target,
        json!({"mcpServers": {"fixture": {"command": "echo"}}, "hasCompletedOnboarding": false})
            .to_string(),
    )
    .unwrap();
    let (plan, _) = preview(ClaudeIntegrationFlag::Onboarding, true).unwrap();
    assert_eq!(plan.changes[0].before.as_deref(), Some("false"));
    assert_eq!(plan.changes[0].after.as_deref(), Some("true"));
    apply(&fixture.root, &plan).unwrap();
    assert_eq!(
        fixture.document(ClaudeIntegrationFlag::Onboarding),
        json!({"mcpServers": {"fixture": {"command": "echo"}}, "hasCompletedOnboarding": true})
    );
    assert!(target
        .with_file_name(".claude.json.asb-lock")
        .symlink_metadata()
        .is_err());
}

#[test]
fn non_object_documents_and_external_changes_never_get_overwritten() {
    let fixture = Fixture::new();
    let target = ClaudeIntegrationFlag::Plugin.target().unwrap();
    std::fs::write(&target, "[1]").unwrap();
    assert!(preview(ClaudeIntegrationFlag::Plugin, true)
        .unwrap_err()
        .contains("不是 JSON 对象"));
    assert!(view(&fixture.root).is_err());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "[1]");

    std::fs::write(&target, "{}").unwrap();
    let (plan, _) = preview(ClaudeIntegrationFlag::Plugin, true).unwrap();
    std::fs::write(&target, json!({"external": 1}).to_string()).unwrap();
    assert!(apply(&fixture.root, &plan)
        .unwrap_err()
        .contains("重新预览"));
    assert_eq!(
        fixture.document(ClaudeIntegrationFlag::Plugin),
        json!({"external": 1})
    );
    assert_eq!(fixture.backups(), 0);
}

#[test]
fn switch_reconciliation_follows_the_policy_and_the_new_route() {
    let fixture = Fixture::new();
    assert_eq!(
        reconcile_after_switch(&fixture.root, RouteMode::Custom),
        Ok(false)
    );
    assert!(!ClaudeIntegrationFlag::Plugin.target().unwrap().exists());

    let saved = save_policy(
        &fixture.root,
        &ClaudeIntegrationPolicy {
            plugin_integration: true,
        },
    )
    .unwrap();
    assert!(saved.policy.plugin_integration);
    assert_eq!(load_policy(&fixture.root).unwrap(), saved.policy);

    assert_eq!(
        reconcile_after_switch(&fixture.root, RouteMode::Custom),
        Ok(true)
    );
    assert_eq!(
        fixture.document(ClaudeIntegrationFlag::Plugin),
        json!({"primaryApiKey": "any"})
    );
    assert_eq!(
        reconcile_after_switch(&fixture.root, RouteMode::Custom),
        Ok(false)
    );
    assert_eq!(
        reconcile_after_switch(&fixture.root, RouteMode::Official),
        Ok(true)
    );
    assert_eq!(fixture.document(ClaudeIntegrationFlag::Plugin), json!({}));

    std::fs::write(fixture.root.join(POLICY_FILE), "{\"unknown\":true}").unwrap();
    assert!(load_policy(&fixture.root).unwrap_err().contains("无效"));
}
