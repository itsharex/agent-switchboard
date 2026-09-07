#![cfg(test)]

use super::*;
use asb_core::extensions::contracts::{
    ExtensionDefinition, McpCheckResult, ProjectRegistration, EXTENSIONS_SCHEMA_VERSION,
};
use asb_core::extensions::skill::ContentEntry;

use asb_core::extensions::contracts::ExtensionKind;
use tempfile::TempDir;

fn store() -> (TempDir, ExtensionStore) {
    let dir = TempDir::new().unwrap();
    let store = ExtensionStore::from_root(dir.path().join("state"));
    (dir, store)
}

fn definition(id: &str) -> ExtensionDefinition {
    serde_json::from_value(serde_json::json!({
        "schemaVersion": EXTENSIONS_SCHEMA_VERSION,
        "id": id,
        "name": "文档检索",
        "revision": 1,
        "createdAt": "2026-09-06T00:00:00Z",
        "updatedAt": "2026-09-06T00:00:00Z",
        "kind": "mcp",
        "transport": "stdio",
        "command": "npx",
    }))
    .unwrap()
}

#[test]
fn definitions_round_trip_strictly() {
    let (_dir, store) = store();
    let definition = definition("mcp-1");
    store.create_definition(&definition).unwrap();
    let list = store.list_definitions().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].kind(), ExtensionKind::Mcp);

    // A foreign shape is rejected, not tolerated.
    let path = store.path(&["definitions", "bad.json"]);
    std::fs::write(&path, r#"{"id":"bad"}"#).unwrap();
    assert!(store.list_definitions().is_err());
    std::fs::remove_file(path).unwrap();

    // A structurally valid previous schema is also rejected. Only the
    // startup migrator may change persisted extension contracts.
    let old = serde_json::json!({
        "schemaVersion": 2,
        "id": "old-definition",
        "name": "旧定义",
        "revision": 1,
        "createdAt": "2026-09-06T00:00:00Z",
        "updatedAt": "2026-09-06T00:00:00Z",
        "kind": "mcp",
        "transport": "stdio",
        "command": "npx"
    });
    std::fs::write(store.path(&["definitions", "old.json"]), old.to_string()).unwrap();
    assert!(store.list_definitions().is_err());
}

#[test]
fn every_metadata_mutation_increases_generation_once() {
    let (_dir, store) = store();
    assert_eq!(store.manifest().unwrap().generation, 0);
    let mut definition = definition("mcp-1");
    store.create_definition(&definition).unwrap();
    assert_eq!(store.manifest().unwrap().generation, 1);
    definition.revision = 2;
    store.update_definition(&definition, 1).unwrap();
    assert_eq!(store.manifest().unwrap().generation, 2);
    assert_eq!(
        store.manifest().unwrap().schema_version,
        EXTENSIONS_SCHEMA_VERSION
    );
}

#[test]
fn skill_versions_are_immutable_and_complete() {
    let (_dir, store) = store();
    let entries = vec![
        ContentEntry {
            relative_path: "SKILL.md".to_string(),
            kind: asb_core::extensions::validate::ContentEntryKind::File,
            bytes: b"hello".to_vec(),
            mode: 0o644,
        },
        ContentEntry {
            relative_path: "scripts".to_string(),
            kind: asb_core::extensions::validate::ContentEntryKind::Dir,
            bytes: Vec::new(),
            mode: 0o755,
        },
        // An empty file stays a file (audit F11).
        ContentEntry {
            relative_path: "empty.md".to_string(),
            kind: asb_core::extensions::validate::ContentEntryKind::File,
            bytes: Vec::new(),
            mode: 0o644,
        },
    ];
    let digest = asb_core::extensions::skill::content_digest(&entries);
    store.save_skill_version("r-1", &digest, &entries).unwrap();
    let loaded = store.load_skill_version("r-1", &digest).unwrap();
    assert_eq!(loaded, entries);
    // Publishing the same content again verifies instead of rewriting.
    store.save_skill_version("r-1", &digest, &entries).unwrap();
    // A digest that does not match its content never publishes.
    let error = store
        .save_skill_version("r-1", &format!("x{digest}"), &entries)
        .unwrap_err();
    assert!(matches!(error, ExtensionStoreError::Conflict(_)));
}

#[test]
fn skill_version_indexes_reject_noncurrent_or_misnamed_content_trees() {
    let (_dir, store) = store();
    let entries = vec![ContentEntry {
        relative_path: "SKILL.md".to_string(),
        kind: asb_core::extensions::validate::ContentEntryKind::File,
        bytes: b"hello".to_vec(),
        mode: 0o644,
    }];
    let digest = asb_core::extensions::skill::content_digest(&entries);
    store.save_skill_version("r-1", &digest, &entries).unwrap();
    let manifest_path = store
        .path(&["library", "r-1", &digest])
        .join(".asb-content.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest["schemaVersion"] = serde_json::json!(EXTENSIONS_SCHEMA_VERSION - 1);
    std::fs::write(&manifest_path, manifest.to_string()).unwrap();

    assert!(store.list_skill_versions("r-1").is_err());
    assert!(store.skill_version_exists("r-1", &digest).is_err());

    manifest["schemaVersion"] = serde_json::json!(EXTENSIONS_SCHEMA_VERSION);
    std::fs::write(&manifest_path, manifest.to_string()).unwrap();
    let wrong_digest = "f".repeat(64);
    std::fs::rename(
        store.path(&["library", "r-1", &digest]),
        store.path(&["library", "r-1", &wrong_digest]),
    )
    .unwrap();

    assert!(store.list_skill_versions("r-1").is_err());
    assert!(store.skill_version_exists("r-1", &wrong_digest).is_err());
}

/// Audit F01: the batch commit must not deadlock against the library
/// lock — the deadlock the old command-layer-held-lock path produced.
#[test]
fn batch_commit_completes_under_its_own_lock() {
    let (_dir, store) = store();
    let binding = ExtensionBinding {
        schema_version: EXTENSIONS_SCHEMA_VERSION,
        id: "bind-1".to_string(),
        resource_id: "r-1".to_string(),
        target: serde_json::from_value(serde_json::json!({
            "scope": "app", "client": "codex"
        }))
        .unwrap(),
        native_key: Some("docs".to_string()),
        deploy_name: None,
        desired: asb_core::extensions::contracts::DesiredState::Enabled,
        locked_digest: None,
        last_applied_revision: None,
        updated_at: "2026-09-06T00:00:00Z".to_string(),
    };
    let commit = LibraryCommit {
        history_operation_id: "op-1".to_string(),
        binding_upserts: vec![binding.clone()],
        baseline_files: vec![(
            binding.id.clone(),
            ManagedBaselineFile::new(&binding.id, Vec::new()),
        )],
        baseline_deletes: Vec::new(),
        binding_deletes: Vec::new(),
        snapshot: None,
        pre_bindings: Vec::new(),
        pre_baselines: Vec::new(),
    };
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signal = done.clone();
    let watcher = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !signal.load(std::sync::atomic::Ordering::SeqCst) {
            if std::time::Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        true
    });
    store.commit_batch(&commit).unwrap();
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(watcher.join().unwrap(), "commit_batch deadlocked");

    assert!(store.has_commit_marker("op-1").unwrap());
    assert_eq!(store.list_bindings().unwrap(), vec![binding]);
}

#[test]
fn historical_restore_recreates_a_binding_after_its_definition_was_removed() {
    let (_dir, store) = store();
    let definition = definition("r-1");
    store.create_definition(&definition).unwrap();
    let binding = ExtensionBinding {
        schema_version: EXTENSIONS_SCHEMA_VERSION,
        id: "bind-1".to_string(),
        resource_id: definition.id.clone(),
        target: serde_json::from_value(serde_json::json!({
            "scope": "app", "client": "codex"
        }))
        .unwrap(),
        native_key: Some("docs".to_string()),
        deploy_name: None,
        desired: asb_core::extensions::contracts::DesiredState::Enabled,
        locked_digest: None,
        last_applied_revision: None,
        updated_at: "2026-09-06T00:00:00Z".to_string(),
    };
    let baseline = ManagedBaselineFile::new(&binding.id, Vec::new());
    store
        .commit_batch(&LibraryCommit {
            history_operation_id: "op-install".to_string(),
            binding_upserts: vec![binding.clone()],
            baseline_files: vec![(binding.id.clone(), baseline.clone())],
            baseline_deletes: Vec::new(),
            binding_deletes: Vec::new(),
            snapshot: None,
            pre_bindings: Vec::new(),
            pre_baselines: Vec::new(),
        })
        .unwrap();
    store
        .commit_batch(&LibraryCommit {
            history_operation_id: "op-remove".to_string(),
            binding_upserts: Vec::new(),
            baseline_files: Vec::new(),
            baseline_deletes: Vec::new(),
            binding_deletes: vec![binding.id.clone()],
            snapshot: None,
            pre_bindings: vec![binding.clone()],
            pre_baselines: vec![(binding.id.clone(), baseline.clone())],
        })
        .unwrap();
    store.delete_definition_if_unbound(&definition.id).unwrap();
    let generation = store.manifest().unwrap().generation;

    store
        .commit_restore_if_current(
            &LibraryCommit {
                history_operation_id: "op-restore".to_string(),
                binding_upserts: vec![binding.clone()],
                baseline_files: vec![(binding.id.clone(), baseline.clone())],
                baseline_deletes: Vec::new(),
                binding_deletes: Vec::new(),
                snapshot: None,
                pre_bindings: Vec::new(),
                pre_baselines: Vec::new(),
            },
            generation,
        )
        .unwrap();

    assert_eq!(store.list_bindings().unwrap(), vec![binding]);
    assert_eq!(store.get_baseline_file("bind-1").unwrap(), Some(baseline));
    assert!(store.get_definition("r-1").unwrap().is_none());
}

#[test]
fn checks_keep_the_latest_per_target() {
    let (_dir, store) = store();
    let target: asb_core::extensions::contracts::ExtensionTarget =
        serde_json::from_value(serde_json::json!({
            "scope": "app",
            "client": "codex"
        }))
        .unwrap();
    let check = |at: &str| McpCheckResult {
        definition_id: "r-1".to_string(),
        definition_revision: 1,
        target: target.clone(),
        checked_at: at.to_string(),
        protocol_version: None,
        outcome: serde_json::from_value(serde_json::json!({
            "kind": "passed", "tools": 1, "resources": 0, "prompts": 0
        }))
        .unwrap(),
        duration_ms: 10,
        truncated: false,
    };
    store.save_check(&check("2026-09-06T00:01:00Z")).unwrap();
    store.save_check(&check("2026-09-06T00:02:00Z")).unwrap();
    let checks = store.list_checks().unwrap();
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].checked_at, "2026-09-06T00:02:00Z");
}

#[test]
fn metadata_creators_reject_a_previous_schema() {
    let (_dir, store) = store();
    let project = ProjectRegistration {
        schema_version: EXTENSIONS_SCHEMA_VERSION - 1,
        id: "project-legacy".to_string(),
        root: "C:/work/project".to_string(),
        display_name: "project".to_string(),
        registered_at: "2026-09-07T00:00:00Z".to_string(),
    };

    assert!(store.create_project(&project).is_err());
    assert!(!store.path(&["projects", "project-legacy.json"]).exists());
}

#[test]
fn production_store_places_extension_backups_in_the_shared_state_backup_root() {
    let dir = TempDir::new().unwrap();
    let state_root = dir.path().join("state");
    let state = crate::local_state::LocalState::from_root(state_root.clone());
    let store = ExtensionStore::from_state(&state);

    assert_eq!(
        store.backup_root("op-1"),
        state_root.join("backups").join("extensions").join("op-1")
    );
}
