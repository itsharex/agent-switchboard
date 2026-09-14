use super::*;
use std::fs;
fn draft(name: &str, content: &str) -> PromptDraft {
    PromptDraft {
        name: name.into(),
        description: None,
        content: content.into(),
    }
}
fn add(root: &Path, target: &Path, name: &str, content: &str) -> PromptsView {
    let revision = list(root, target).unwrap().revision;
    save(
        root,
        target,
        &root.join("backups"),
        None,
        draft(name, content),
        &revision,
        None,
        false,
    )
    .unwrap()
}
fn apply(root: &Path, target: &Path, id: Option<String>) -> PromptsView {
    let revision = list(root, target).unwrap().revision;
    let plan = preview(root, target, id, &revision).unwrap().plan;
    activate(root, target, &root.join("backups"), plan).unwrap()
}
#[test]
fn inactive_crud_is_isolated_and_stale_writes_do_not_change_the_library() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let target = root.join("client/AGENTS.md");
    let first = add(root, &target, "One", "first");
    let id = first.presets[0].id.clone();
    assert!(!target.exists());
    let second = add(root, &target, "Two", "second");
    assert!(save(
        root,
        &target,
        &root.join("backups"),
        Some(&id),
        draft("stale", "wrong"),
        &first.revision,
        None,
        false
    )
    .is_err());
    let updated = delete(root, &target, &id, &second.revision).unwrap();
    assert_eq!(updated.presets.len(), 1);
    assert!(!target.exists());
    assert!(!root.join("CLAUDE.md").exists());
}
#[test]
fn activation_backs_up_unmanaged_live_and_switching_backfills_even_empty_external_edits() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let target = root.join("AGENTS.md");
    fs::write(&target, "original instructions").unwrap();
    let first = add(root, &target, "One", "first");
    let first_id = first.presets[0].id.clone();
    let second = add(root, &target, "Two", "second");
    let second_id = second.presets[1].id.clone();
    let activated = apply(root, &target, Some(first_id.clone()));
    assert_eq!(fs::read_to_string(&target).unwrap(), "first");
    assert!(activated
        .presets
        .iter()
        .any(|p| p.draft.content == "original instructions"));
    assert!(delete(root, &target, &first_id, &activated.revision).is_err());
    fs::write(&target, "").unwrap();
    let next = apply(root, &target, Some(second_id));
    assert_eq!(
        next.presets
            .iter()
            .find(|p| p.id == first_id)
            .unwrap()
            .draft
            .content,
        ""
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "second");
    let stopped = apply(root, &target, None);
    assert!(stopped.active_id.is_none());
    assert_eq!(fs::read_to_string(&target).unwrap(), "");
    assert!(stopped.presets.iter().any(|p| p.draft.content == "second"));
    assert!(fs::read_dir(root.join("backups")).unwrap().count() >= 6);
}
#[test]
fn active_edit_requires_confirmation_and_detects_external_changes() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let target = root.join("AGENTS.md");
    let first = add(root, &target, "One", "first");
    let id = first.presets[0].id.clone();
    let active = apply(root, &target, Some(id.clone()));
    assert!(save(
        root,
        &target,
        &root.join("backups"),
        Some(&id),
        draft("One", "new"),
        &active.revision,
        Some(&active.live.content_hash),
        false
    )
    .is_err());
    fs::write(&target, "external").unwrap();
    assert!(save(
        root,
        &target,
        &root.join("backups"),
        Some(&id),
        draft("One", "new"),
        &active.revision,
        Some(&active.live.content_hash),
        true
    )
    .is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), "external");
    assert_eq!(list(root, &target).unwrap().revision, active.revision);
    let same = apply(root, &target, Some(id.clone()));
    assert_eq!(fs::read_to_string(&target).unwrap(), "external");
    let saved = save(
        root,
        &target,
        &root.join("backups"),
        Some(&id),
        draft("renamed", "new"),
        &same.revision,
        Some(&same.live.content_hash),
        true,
    )
    .unwrap();
    assert_eq!(saved.presets[0].draft.content, "new");
    assert_eq!(fs::read_to_string(&target).unwrap(), "new");
}
#[test]
fn preview_is_pure_and_cannot_be_reused_after_library_or_live_edits() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let target = root.join("AGENTS.md");
    let first = add(root, &target, "One", "first");
    let id = first.presets[0].id.clone();
    let prepared = preview(root, &target, Some(id), &first.revision).unwrap();
    assert!(!target.exists());
    add(root, &target, "Other", "other");
    assert!(activate(root, &target, &root.join("backups"), prepared.plan).is_err());
    assert!(!target.exists());
}
#[test]
fn restart_recovery_completes_only_the_recorded_live_version_and_preserves_external_edits() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let target = root.join("AGENTS.md");
    let initial = add(root, &target, "One", "first");
    let before = store::load(root).unwrap().0;
    let mut after = before.clone();
    after.active_id = Some(initial.presets[0].id.clone());
    let intent = serde_json::json!({"version":1,"before":before,"after":after,"beforeLiveHash":asb_switch::sha256_hex(""),"beforeLiveExisted":false,"afterLiveHash":asb_switch::sha256_hex("first")});
    fs::write(transaction::pending(root), intent.to_string()).unwrap();
    fs::write(&target, "outside").unwrap();
    assert!(recover(root, &target).unwrap_err().contains("外改"));
    assert_eq!(fs::read_to_string(&target).unwrap(), "outside");
    assert!(transaction::pending(root).exists());
    fs::write(&target, "first").unwrap();
    let repaired = recover(root, &target).unwrap();
    assert_eq!(repaired.active_id, after.active_id);
    assert!(!repaired.pending_recovery);
}
#[test]
fn prompt_recovery_only_removes_dead_process_locks() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let target = root.join("AGENTS.md");
    fs::write(&target, "kept").unwrap();
    let lock = asb_switch::lockfile::lock_path_for(&target);
    let data = |pid| {
        serde_json::to_string(&asb_core::LockFileData {
            pid: Some(pid),
            process_name: Some("fixture".into()),
            acquired_at: None,
        })
        .unwrap()
    };
    fs::write(&lock, data(std::process::id())).unwrap();
    assert!(recover(root, &target).is_err());
    assert!(lock.exists());
    fs::write(&lock, data(u32::MAX)).unwrap();
    assert!(list(root, &target).unwrap().pending_recovery);
    assert!(!recover(root, &target).unwrap().pending_recovery);
    assert!(!lock.exists());
    assert_eq!(fs::read_to_string(&target).unwrap(), "kept");
}
