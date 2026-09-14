use asb_core::extensions::contracts::EXTENSIONS_SCHEMA_VERSION;
use asb_core::extensions::skill::{content_digest, ContentEntry};
use asb_core::extensions::validate::ContentEntryKind;
use serde_json::json;

use super::*;

fn fixture() -> (tempfile::TempDir, ExtensionStore, ExtensionDefinition) {
    let temp = tempfile::tempdir().unwrap();
    let store = ExtensionStore::from_root(temp.path().join("extensions"));
    let entries = vec![ContentEntry {
        relative_path: "SKILL.md".into(), kind: ContentEntryKind::File,
        bytes: b"---\nname: backup-helper\ndescription: A recoverable skill\n---\nOriginal instructions".to_vec(),
        mode: 0o644,
    }];
    let definition: ExtensionDefinition = serde_json::from_value(json!({
        "schemaVersion": EXTENSIONS_SCHEMA_VERSION, "id": "ext-backup-helper",
        "name": "backup-helper", "revision": 3,
        "createdAt": "2026-09-12T00:00:00Z", "updatedAt": "2026-09-12T01:00:00Z",
        "kind": "skill", "contentDigest": content_digest(&entries),
        "manifest": {"name": "backup-helper", "description": "A recoverable skill"},
        "source": {"sourceId": "org/repo", "subpath": "skills/helper", "refName": "main",
            "resolvedCommit": "0123456789abcdef0123456789abcdef01234567"}
    }))
    .unwrap();
    let ExtensionPayload::Skill(skill) = &definition.payload else {
        unreachable!()
    };
    store
        .save_skill_version(&definition.id, &skill.content_digest, &entries)
        .unwrap();
    store.create_definition(&definition).unwrap();
    (temp, store, definition)
}

#[test]
fn deleted_skill_restores_identity_source_and_verified_content_without_deploying() {
    let (_temp, store, definition) = fixture();
    store.delete_definition_if_unbound(&definition.id).unwrap();
    assert!(store.get_definition(&definition.id).unwrap().is_none());
    let backups = store.list_skill_backups().unwrap();
    assert_eq!(backups.len(), 1);
    assert_eq!(backups[0].definition, definition);
    assert_eq!(
        store.restore_skill_backup(&backups[0].id).unwrap(),
        definition
    );
    assert_eq!(
        store.restore_skill_backup(&backups[0].id).unwrap(),
        definition
    );
    assert!(store.list_bindings().unwrap().is_empty());
    assert!(store.list_history().unwrap().is_empty());
}

#[test]
fn damaged_backup_content_is_not_restored_or_silently_recreated() {
    let (_temp, store, definition) = fixture();
    store.delete_definition_if_unbound(&definition.id).unwrap();
    let backup = store.list_skill_backups().unwrap().remove(0);
    let ExtensionPayload::Skill(skill) = &definition.payload else {
        unreachable!()
    };
    fs::write(
        store.path(&["library", &definition.id, &skill.content_digest, "SKILL.md"]),
        "changed",
    )
    .unwrap();
    assert!(store.restore_skill_backup(&backup.id).is_err());
    assert!(store.get_definition(&definition.id).unwrap().is_none());
    assert_eq!(store.list_skill_backups().unwrap().len(), 1);
}

#[test]
fn restore_never_overwrites_a_changed_existing_definition() {
    let (_temp, store, definition) = fixture();
    store.delete_definition_if_unbound(&definition.id).unwrap();
    let backup = store.list_skill_backups().unwrap().remove(0);
    store.restore_skill_backup(&backup.id).unwrap();
    let mut edited = definition.clone();
    edited.name = "renamed-helper".into();
    edited.revision += 1;
    store
        .update_definition(&edited, definition.revision)
        .unwrap();
    assert!(matches!(
        store.restore_skill_backup(&backup.id),
        Err(ExtensionStoreError::Conflict(_))
    ));
    assert_eq!(store.get_definition(&definition.id).unwrap(), Some(edited));
}

#[test]
fn removing_backup_retains_content_referenced_by_history() {
    let (_temp, store, definition) = fixture();
    store.delete_definition_if_unbound(&definition.id).unwrap();
    let backup = store.list_skill_backups().unwrap().remove(0);
    store.delete_skill_backup(&backup.id).unwrap();
    assert!(store.list_skill_backups().unwrap().is_empty());
    let ExtensionPayload::Skill(skill) = &definition.payload else {
        unreachable!()
    };
    assert!(store
        .skill_version_exists(&definition.id, &skill.content_digest)
        .unwrap());
    assert!(store
        .delete_skill_backup("../../definitions/ext-backup-helper")
        .is_err());
}

#[test]
fn backup_failure_keeps_the_definition_and_generation() {
    let (_temp, store, definition) = fixture();
    let generation = store.manifest().unwrap().generation;
    fs::write(store.path(&["skill-backups"]), "not a directory").unwrap();
    assert!(store.delete_definition_if_unbound(&definition.id).is_err());
    assert_eq!(
        store.get_definition(&definition.id).unwrap(),
        Some(definition)
    );
    assert_eq!(store.manifest().unwrap().generation, generation);
}

#[test]
fn installed_skill_cannot_be_deleted_before_its_bindings_are_removed() {
    let (_temp, store, definition) = fixture();
    store.write_json(&store.path(&["bindings", "binding-backup.json"]), &json!({
        "schemaVersion": EXTENSIONS_SCHEMA_VERSION, "id": "binding-backup",
        "resourceId": definition.id, "target": {"scope": "app", "client": "codex"},
        "deployName": "backup-helper", "desired": "enabled", "updatedAt": "2026-09-12T00:00:00Z"
    })).unwrap();
    assert!(matches!(
        store.delete_definition_if_unbound(&definition.id),
        Err(ExtensionStoreError::Conflict(_))
    ));
    assert!(store.get_definition(&definition.id).unwrap().is_some());
    assert!(store.list_skill_backups().unwrap().is_empty());
}
