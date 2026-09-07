//! Rename plan tests: one-document native-key moves, collision and
//! external-change rejections.
#![cfg(test)]

use super::super::{PlanRequest, PlanRequestOperation};
use super::*;
use asb_core::extensions::contracts::{SkillDefinition, SkillManifest};
use std::collections::BTreeMap;

use crate::extensions::discovery::DiscoveredPaths;
use crate::extensions::store::{ExtensionStore, LibraryCommit};
use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::McpDefinition;
use asb_core::extensions::contracts::{DesiredState, ExtensionTarget, EXTENSIONS_SCHEMA_VERSION};
use tempfile::TempDir;

pub(super) fn stdio_definition(id: &str, name: &str, revision: u64) -> ExtensionDefinition {
    ExtensionDefinition {
        schema_version: EXTENSIONS_SCHEMA_VERSION,
        id: id.to_string(),
        name: name.to_string(),
        revision,
        created_at: "2026-09-06T00:00:00.000Z".to_string(),
        updated_at: "2026-09-06T00:00:00.000Z".to_string(),
        payload: ExtensionPayload::Mcp(McpDefinition::Stdio {
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "server".to_string()],
            env: BTreeMap::new(),
            codex_options: None,
        }),
    }
}

pub(super) fn binding(
    id: &str,
    resource: &str,
    key: &str,
    desired: DesiredState,
) -> ExtensionBinding {
    ExtensionBinding {
        schema_version: EXTENSIONS_SCHEMA_VERSION,
        id: id.to_string(),
        resource_id: resource.to_string(),
        target: ExtensionTarget::App {
            client: AppKind::Codex,
        },
        native_key: Some(key.to_string()),
        deploy_name: None,
        desired,
        locked_digest: None,
        last_applied_revision: Some(1),
        updated_at: "2026-09-06T00:00:00.000Z".to_string(),
    }
}

pub(super) fn write_config(home: &std::path::Path, text: &str) -> std::path::PathBuf {
    let config = home.join(".codex").join("config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, text).unwrap();
    config
}

/// Home with one deployed Codex server `docs` (definition `ext-1`) whose
/// binding baseline is current; the definition is then renamed the way
/// `update_mcp_definition` persists it.
pub(super) fn renamed_workspace(
    config_text: &str,
    extra_commit: Option<(ExtensionDefinition, ExtensionBinding, String)>,
) -> (TempDir, ExtensionStore, DiscoveredPaths) {
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), config_text);
    let store = ExtensionStore::from_root(dir.path().join("state"));
    let definition = stdio_definition("ext-1", "docs", 1);
    store.create_definition(&definition).unwrap();
    let bind = binding("bind-1", "ext-1", "docs", DesiredState::Enabled);
    let baseline = ManagedBaselineFile::new(
        "bind-1",
        vec![ManagedBaseline::DocumentEntry {
            target_path: config.to_string_lossy().to_string(),
            entry_pointer: "mcp_servers.docs".to_string(),
            original_value: None,
            last_written_value: Some("written".to_string()),
            last_document_hash: sha_hex(config_text.as_bytes()),
            target_existed_before: true,
            backup_reference: None,
        }],
    );
    let mut upserts = vec![bind];
    let mut baselines = vec![("bind-1".to_string(), baseline)];
    if let Some((other_definition, other_binding, other_baseline_pointer)) = extra_commit {
        store.create_definition(&other_definition).unwrap();
        let other_baseline = ManagedBaselineFile::new(
            &other_binding.id,
            vec![ManagedBaseline::DocumentEntry {
                target_path: config.to_string_lossy().to_string(),
                entry_pointer: other_baseline_pointer,
                original_value: None,
                last_written_value: Some("written".to_string()),
                last_document_hash: sha_hex(config_text.as_bytes()),
                target_existed_before: true,
                backup_reference: None,
            }],
        );
        baselines.push((other_binding.id.clone(), other_baseline));
        upserts.push(other_binding);
    }
    store
        .commit_batch(&LibraryCommit {
            history_operation_id: "op-seed".to_string(),
            binding_upserts: upserts,
            baseline_files: baselines,
            baseline_deletes: Vec::new(),
            binding_deletes: Vec::new(),
            snapshot: None,
            pre_bindings: Vec::new(),
            pre_baselines: Vec::new(),
        })
        .unwrap();
    let mut renamed = stdio_definition("ext-1", "docs_v2", 2);
    renamed.revision = 2;
    renamed.name = "docs_v2".to_string();
    store.update_definition(&renamed, 1).unwrap();
    let paths = DiscoveredPaths {
        home: dir.path().to_path_buf(),
        codex_home: None,
        claude_dir: None,
    };
    (dir, store, paths)
}

pub(super) fn update_request(definition_id: &str) -> PlanRequest {
    PlanRequest {
        operations: vec![PlanRequestOperation {
            operation: PlanOperation::Update,
            definition_id: Some(definition_id.to_string()),
            binding_id: None,
            targets: Vec::new(),
            shared_settings: None,
        }],
    }
}

const DEPLOYED_CONFIG: &str = r#"[mcp_servers.docs]
command = "npx"
args = ["-y", "server"]

# host-owned section survives
[other]
value = 1
"#;

#[test]
pub(super) fn rename_update_moves_the_native_key_in_one_document_step() {
    let (_dir, store, paths) = renamed_workspace(DEPLOYED_CONFIG, None);
    let secrets = |_: &str| -> Option<String> { None };
    let planner = Planner {
        store: &store,
        paths: &paths,
        secrets: &secrets,
    };
    let plan = planner.build(&update_request("ext-1")).unwrap();
    assert_eq!(plan.operations[0].targets.len(), 1);
    let target = &plan.operations[0].targets[0];
    assert_eq!(target.binding.native_key.as_deref(), Some("docs_v2"));
    assert_eq!(target.steps.len(), 1);
    let rendered = match &target.steps[0] {
        PlanStep::DocumentWrite { rendered, .. } => rendered.clone(),
        other => panic!("expected one document write, got {other:?}"),
    };
    assert!(rendered.contains("mcp_servers.docs_v2"));
    assert!(!rendered.contains("mcp_servers.docs]"));
    assert!(rendered.contains("other"));
    assert!(target
        .changes
        .iter()
        .any(|change| change.pointer == "mcp_servers.docs" && change.after.is_none()));
    assert!(target
        .changes
        .iter()
        .any(|change| change.pointer == "mcp_servers.docs_v2" && change.after.is_some()));
    let baseline = target
        .baseline
        .as_ref()
        .expect("rename settles the baseline");
    assert!(
            baseline
                .entries
                .iter()
                .any(|entry| matches!(entry, ManagedBaseline::DocumentEntry { entry_pointer, .. } if entry_pointer == "mcp_servers.docs_v2"))
        );
    assert!(
            !baseline
                .entries
                .iter()
                .any(|entry| matches!(entry, ManagedBaseline::DocumentEntry { entry_pointer, .. } if entry_pointer == "mcp_servers.docs"))
        );
}

/// A skill binding pinned to an older content version survives an
/// update plan untouched: the plan carries a zero-step target whose
/// warning names the pin, never a silent redeploy.
#[test]
pub(super) fn redeploy_reports_a_pinned_binding_instead_of_moving_it() {
    let dir = TempDir::new().unwrap();
    let store = ExtensionStore::from_root(dir.path().join("state"));
    let definition = ExtensionDefinition {
        schema_version: EXTENSIONS_SCHEMA_VERSION,
        id: "ext-skill-1".to_string(),
        name: "api-spec".to_string(),
        revision: 3,
        created_at: "2026-09-06T00:00:00.000Z".to_string(),
        updated_at: "2026-09-06T00:00:00.000Z".to_string(),
        payload: ExtensionPayload::Skill(SkillDefinition {
            content_digest: "new-digest".to_string(),
            manifest: SkillManifest {
                name: "api-spec".to_string(),
                description: Some("API 规范辅助 Skill".to_string()),
                license: None,
                allowed_tools: None,
                unparsed_keys: Vec::new(),
            },
            source: None,
            host_scoped: None,
            compatibility: Vec::new(),
            dependencies: Vec::new(),
        }),
    };
    store.create_definition(&definition).unwrap();
    let pinned = ExtensionBinding {
        schema_version: EXTENSIONS_SCHEMA_VERSION,
        id: "bind-pin".to_string(),
        resource_id: "ext-skill-1".to_string(),
        target: ExtensionTarget::App {
            client: AppKind::Codex,
        },
        native_key: None,
        deploy_name: Some("api-spec".to_string()),
        desired: DesiredState::Enabled,
        locked_digest: Some("old-digest".to_string()),
        last_applied_revision: Some(1),
        updated_at: "2026-09-06T00:00:00.000Z".to_string(),
    };
    store
        .commit_batch(&LibraryCommit {
            history_operation_id: "op-seed-pin".to_string(),
            binding_upserts: vec![pinned],
            baseline_files: Vec::new(),
            baseline_deletes: Vec::new(),
            binding_deletes: Vec::new(),
            snapshot: None,
            pre_bindings: Vec::new(),
            pre_baselines: Vec::new(),
        })
        .unwrap();
    let paths = DiscoveredPaths {
        home: dir.path().to_path_buf(),
        codex_home: None,
        claude_dir: None,
    };
    let secrets = |_: &str| -> Option<String> { None };
    let planner = Planner {
        store: &store,
        paths: &paths,
        secrets: &secrets,
    };
    let plan = planner.build(&update_request("ext-skill-1")).unwrap();
    assert_eq!(plan.operations.len(), 1);
    let targets = &plan.operations[0].targets;
    assert_eq!(targets.len(), 1);
    assert!(targets[0].steps.is_empty());
    assert!(targets[0].changes.is_empty());
    assert!(targets[0].baseline.is_none());
    assert!(targets[0]
        .warnings
        .iter()
        .any(|warning| warning.contains("已固定版本 old-diges")));
    assert_eq!(
        targets[0].binding.locked_digest.as_deref(),
        Some("old-digest")
    );
}

/// A disabled binding is skipped by update plans exactly as before,
/// whether or not it carries a pin: nothing deploys, so the pin adds
/// no preview noise.
#[test]
pub(super) fn redeploy_skips_a_disabled_pinned_binding_entirely() {
    let dir = TempDir::new().unwrap();
    let store = ExtensionStore::from_root(dir.path().join("state"));
    let definition = ExtensionDefinition {
        schema_version: EXTENSIONS_SCHEMA_VERSION,
        id: "ext-skill-1".to_string(),
        name: "api-spec".to_string(),
        revision: 3,
        created_at: "2026-09-06T00:00:00.000Z".to_string(),
        updated_at: "2026-09-06T00:00:00.000Z".to_string(),
        payload: ExtensionPayload::Skill(SkillDefinition {
            content_digest: "new-digest".to_string(),
            manifest: SkillManifest {
                name: "api-spec".to_string(),
                description: Some("API 规范辅助 Skill".to_string()),
                license: None,
                allowed_tools: None,
                unparsed_keys: Vec::new(),
            },
            source: None,
            host_scoped: None,
            compatibility: Vec::new(),
            dependencies: Vec::new(),
        }),
    };
    store.create_definition(&definition).unwrap();
    let pinned = ExtensionBinding {
        schema_version: EXTENSIONS_SCHEMA_VERSION,
        id: "bind-pin-off".to_string(),
        resource_id: "ext-skill-1".to_string(),
        target: ExtensionTarget::App {
            client: AppKind::Codex,
        },
        native_key: None,
        deploy_name: Some("api-spec".to_string()),
        desired: DesiredState::Disabled,
        locked_digest: Some("old-digest".to_string()),
        last_applied_revision: Some(1),
        updated_at: "2026-09-06T00:00:00.000Z".to_string(),
    };
    store
        .commit_batch(&LibraryCommit {
            history_operation_id: "op-seed-pin-off".to_string(),
            binding_upserts: vec![pinned],
            baseline_files: Vec::new(),
            baseline_deletes: Vec::new(),
            binding_deletes: Vec::new(),
            snapshot: None,
            pre_bindings: Vec::new(),
            pre_baselines: Vec::new(),
        })
        .unwrap();
    let paths = DiscoveredPaths {
        home: dir.path().to_path_buf(),
        codex_home: None,
        claude_dir: None,
    };
    let secrets = |_: &str| -> Option<String> { None };
    let planner = Planner {
        store: &store,
        paths: &paths,
        secrets: &secrets,
    };
    let plan = planner.build(&update_request("ext-skill-1")).unwrap();
    assert_eq!(plan.operations.len(), 1);
    assert!(
        plan.operations[0].targets.is_empty(),
        "disabled bindings carry no update targets, pinned or not"
    );
}

#[test]
pub(super) fn rename_to_a_native_entry_in_the_document_is_rejected() {
    let config = format!("{DEPLOYED_CONFIG}[mcp_servers.docs_v2]\ncommand = \"other\"\n");
    let (_dir, store, paths) = renamed_workspace(&config, None);
    let secrets = |_: &str| -> Option<String> { None };
    let planner = Planner {
        store: &store,
        paths: &paths,
        secrets: &secrets,
    };
    let error = planner.build(&update_request("ext-1")).unwrap_err();
    assert_eq!(error.code, "extension-conflict");
    assert!(error.message.contains("同名原生"));
}

#[test]
pub(super) fn rename_to_a_key_owned_by_another_binding_is_rejected() {
    // `ext-2` owns `docs_v2` in the same document and namespace; its
    // entry need not exist on disk yet for the conflict to matter.
    let other = stdio_definition("ext-2", "docs_v2", 1);
    let other_binding = binding("bind-2", "ext-2", "docs_v2", DesiredState::Enabled);
    let (_dir, store, paths) = renamed_workspace(
        DEPLOYED_CONFIG,
        Some((other, other_binding, "mcp_servers.docs_v2".to_string())),
    );
    let secrets = |_: &str| -> Option<String> { None };
    let planner = Planner {
        store: &store,
        paths: &paths,
        secrets: &secrets,
    };
    let error = planner.build(&update_request("ext-1")).unwrap_err();
    assert_eq!(error.code, "extension-conflict");
    assert!(error.message.contains("另一绑定占用"));
}

#[test]
pub(super) fn external_document_change_blocks_the_rename_plan() {
    let (_dir, store, paths) = renamed_workspace(DEPLOYED_CONFIG, None);
    // The document changes after the baseline was captured.
    let config = paths.home.join(".codex").join("config.toml");
    std::fs::write(&config, format!("{DEPLOYED_CONFIG}# touched\n")).unwrap();
    let secrets = |_: &str| -> Option<String> { None };
    let planner = Planner {
        store: &store,
        paths: &paths,
        secrets: &secrets,
    };
    let error = planner.build(&update_request("ext-1")).unwrap_err();
    assert_eq!(error.code, "extension-external-change");
}
