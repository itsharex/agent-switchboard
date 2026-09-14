use super::*;
use serde_json::json;

fn draft(name: &str, content: &str) -> ClaudePromptDraft {
    ClaudePromptDraft {
        name: name.into(),
        description: None,
        content: content.into(),
    }
}

#[test]
fn claude_prompt_library_roundtrips_crud_order_and_does_not_touch_codex_or_live_on_save() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("CLAUDE.md");
    let codex = dir.path().join("AGENTS.md");
    std::fs::write(&codex, "keep codex").unwrap();
    let empty = list(dir.path(), &target).unwrap();
    let first = save(
        dir.path(),
        &target,
        None,
        draft("工作", "先验证再改动"),
        &empty.file_hash,
    )
    .unwrap();
    assert!(!target.exists());
    assert!(save(
        dir.path(),
        &target,
        None,
        draft("工作", "duplicate"),
        &first.file_hash
    )
    .is_err());
    assert!(save(
        dir.path(),
        &target,
        None,
        draft("过时", "stale"),
        &empty.file_hash
    )
    .is_err());
    let second = save(
        dir.path(),
        &target,
        None,
        draft("阅读", "只解释，不写入"),
        &first.file_hash,
    )
    .unwrap();
    let ids = second
        .prompts
        .iter()
        .rev()
        .map(|prompt| prompt.id.clone())
        .collect::<Vec<_>>();
    let sorted = reorder(dir.path(), &target, &ids, &second.file_hash).unwrap();
    assert_eq!(sorted.prompts[0].draft.name, "阅读");
    let removed = remove(dir.path(), &target, &ids[0], &sorted.file_hash).unwrap();
    assert_eq!(removed.prompts.len(), 1);
    assert_eq!(std::fs::read_to_string(codex).unwrap(), "keep codex");
}

#[test]
fn activation_uses_preview_executor_backup_and_never_silently_reapplies_an_edited_template() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("CLAUDE.md");
    std::fs::write(&target, "用户原文").unwrap();
    let view = save(
        dir.path(),
        &target,
        None,
        draft("工作", "活动提示词"),
        &list(dir.path(), &target).unwrap().file_hash,
    )
    .unwrap();
    let id = view.prompts[0].id.clone();
    let preview = super::preview(dir.path(), &target, Some(id.clone()), &view.file_hash).unwrap();
    assert_eq!(preview.before, "用户原文");
    let view = activate(
        dir.path(),
        &target,
        &dir.path().join("backups"),
        preview.plan,
    )
    .unwrap();
    assert_eq!(view.active_prompt_id, Some(id.clone()));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "活动提示词");
    assert!(std::fs::read_dir(dir.path().join("backups"))
        .unwrap()
        .any(|entry| entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|ext| ext == "bak")));
    let view = save(
        dir.path(),
        &target,
        Some(&id),
        draft("工作", "尚未应用"),
        &view.file_hash,
    )
    .unwrap();
    assert!(view.pending_content);
    assert!(!view.external_change);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "活动提示词");
    assert!(remove(dir.path(), &target, &id, &view.file_hash).is_err());
    let preview = super::preview(dir.path(), &target, None, &view.file_hash).unwrap();
    let view = activate(
        dir.path(),
        &target,
        &dir.path().join("backups"),
        preview.plan,
    )
    .unwrap();
    assert_eq!(view.active_prompt_id, None);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "");
}

#[test]
fn external_edits_and_tampered_previews_are_rejected_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("CLAUDE.md");
    std::fs::write(&target, "first").unwrap();
    let view = save(
        dir.path(),
        &target,
        None,
        draft("test", "next"),
        &list(dir.path(), &target).unwrap().file_hash,
    )
    .unwrap();
    let mut plan = super::preview(
        dir.path(),
        &target,
        Some(view.prompts[0].id.clone()),
        &view.file_hash,
    )
    .unwrap()
    .plan;
    let original = plan.clone();
    plan.rendered_hash = "0".repeat(64);
    assert!(activate(dir.path(), &target, &dir.path().join("backups"), plan).is_err());
    std::fs::write(&target, "外部编辑").unwrap();
    assert!(activate(dir.path(), &target, &dir.path().join("backups"), original).is_err());
    assert_eq!(std::fs::read_to_string(target).unwrap(), "外部编辑");
}

#[test]
fn crash_recovery_finishes_the_native_write_and_preserves_conflicting_external_content() {
    for conflict in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("CLAUDE.md");
        std::fs::write(&target, "before").unwrap();
        let view = save(
            dir.path(),
            &target,
            None,
            draft("test", "after"),
            &list(dir.path(), &target).unwrap().file_hash,
        )
        .unwrap();
        let plan = super::preview(
            dir.path(),
            &target,
            Some(view.prompts[0].id.clone()),
            &view.file_hash,
        )
        .unwrap()
        .plan;
        transaction::interrupt_after_document(dir.path(), &target, &plan, "after");
        assert!(list(dir.path(), &target).unwrap().recovery_required);
        if conflict {
            std::fs::write(&target, "外部编辑不能回滚").unwrap();
            assert!(recover(dir.path(), &target).is_err());
            assert_eq!(
                std::fs::read_to_string(&target).unwrap(),
                "外部编辑不能回滚"
            );
            assert!(list(dir.path(), &target).unwrap().recovery_required);
        } else {
            let view = recover(dir.path(), &target).unwrap();
            assert_eq!(view.active_prompt_id, plan.prompt_id);
            assert!(!view.recovery_required);
            assert!(!view.external_change);
        }
    }
}

#[test]
fn malformed_libraries_and_pending_transactions_are_not_reset() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("CLAUDE.md");
    std::fs::create_dir_all(store::path(dir.path()).parent().unwrap()).unwrap();
    let original = json!({"version":999,"prompts":[],"active":null}).to_string();
    std::fs::write(store::path(dir.path()), &original).unwrap();
    assert!(list(dir.path(), &target).is_err());
    assert!(save(dir.path(), &target, None, draft("test", "data"), "hash").is_err());
    assert_eq!(
        std::fs::read_to_string(store::path(dir.path())).unwrap(),
        original
    );
}

#[test]
fn recovery_accepts_the_exact_valid_source_bytes_even_after_external_json_reformatting() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("CLAUDE.md");
    std::fs::write(&target, "before").unwrap();
    save(
        dir.path(),
        &target,
        None,
        draft("test", "after"),
        &list(dir.path(), &target).unwrap().file_hash,
    )
    .unwrap();
    let source = std::fs::read_to_string(store::path(dir.path())).unwrap();
    let formatted = format!(" \n{source}\n\n");
    std::fs::write(store::path(dir.path()), &formatted).unwrap();
    let view = list(dir.path(), &target).unwrap();
    let plan = super::preview(
        dir.path(),
        &target,
        Some(view.prompts[0].id.clone()),
        &view.file_hash,
    )
    .unwrap()
    .plan;
    transaction::interrupt_after_document(dir.path(), &target, &plan, "after");
    let view = recover(dir.path(), &target).unwrap();
    assert!(!view.recovery_required);
    assert!(!view.external_change);
}
