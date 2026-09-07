//! Crash-recovery tests: journal replay, in-flight step undo, committed
//! cleanup, conflict refusal, and lock-target reconstruction.

mod common;

use asb_core::extensions::contracts::DocumentSyntax;
use asb_switch::extensions::recovery::JournalEntry;
use asb_switch::extensions::transaction::JOURNAL_FILE_NAME;
use asb_switch::{recover_pending, FsIo};
use common::extensions::{fixture, sha, two_target_plan};
use std::fs;

#[test]
fn crash_recovery_undoes_a_completed_document_write_from_the_journal() {
    let fx = fixture("crash-recovery");
    let target = fx._dir.path().join("live").join("config.toml");
    fs::create_dir_all(&fx._dir.path().join("live")).unwrap();
    fs::write(&target, "model = \"gpt-5.2\"\n").unwrap();
    let original = sha(&target);

    // Simulate a crash after the write but before the commit: produce the
    // real artifacts (backup + written file + journal lines) by hand.
    let backup_dir = fx.backup_dir.clone();
    fs::create_dir_all(&backup_dir).unwrap();
    let backup_path = backup_dir.join("config.toml.20260906T000000Z-1.bak");
    fs::write(&backup_path, "model = \"gpt-5.2\"\n").unwrap();
    fs::write(
        &target,
        "model = \"gpt-5.2\"\n\n[mcp_servers.docs]\ncommand = \"npx\"\n",
    )
    .unwrap();
    let written_hash = sha(&target).unwrap();
    let journal = format!(
        "{}\n{}\n",
        serde_json::to_string(&serde_json::json!({
            "phase": "prepared",
            "operationId": "op-1",
            "createdAt": "2026-09-06T00:00:00Z",
            "plan": {},
        }))
        .unwrap(),
        serde_json::to_string(&serde_json::json!({
            "phase": "stepCompleted",
            "target": 0,
            "index": 0,
            "detail": {
                "kind": "documentWritten",
                "path": target.to_string_lossy(),
                "syntax": "toml",
                "backupReference": backup_path.to_string_lossy(),
                "writtenHash": written_hash,
                "originalHash": original,
            },
        }))
        .unwrap(),
    );
    fs::write(fx.journal_dir.join(JOURNAL_FILE_NAME), journal).unwrap();

    let report = recover_pending(&FsIo, &fx.journal_dir).unwrap();
    assert!(!report.completed_cleanly);
    assert_eq!(report.restored.len(), 1, "{report:?}");
    assert!(report.failed.is_empty());
    assert_eq!(sha(&target).as_deref(), original.as_deref());
    assert!(!fx.journal_dir.exists(), "recovered journal is cleaned up");
}

#[test]
fn crash_recovery_of_a_half_directory_swap_restores_the_old_tree() {
    let fx = fixture("crash-swap");
    let skill_root = fx._dir.path().join("home").join(".agents").join("skills");
    let deployed = skill_root.join("api-spec");
    fs::create_dir_all(&deployed).unwrap();
    fs::write(deployed.join("SKILL.md"), b"old").unwrap();

    // Simulate: new tree staged as sibling, old tree moved aside, target
    // missing — the journal holds only the intent.
    let new_dir = skill_root.join("api-spec.424242.asb-ext-new");
    let old_dir = skill_root.join("api-spec.424242.asb-ext-old");
    fs::create_dir_all(&new_dir).unwrap();
    fs::write(new_dir.join("SKILL.md"), b"new").unwrap();
    fs::rename(&deployed, &old_dir).unwrap();
    let journal = serde_json::to_string(&serde_json::json!({
        "phase": "stepStarted",
        "target": 0,
        "index": 0,
        "kind": "deploy",
        "path": deployed.to_string_lossy(),
    }))
    .unwrap();
    fs::create_dir_all(&fx.journal_dir).unwrap();
    fs::write(
        fx.journal_dir.join(JOURNAL_FILE_NAME),
        format!("{journal}\n"),
    )
    .unwrap();

    let report = recover_pending(&FsIo, &fx.journal_dir).unwrap();
    assert_eq!(report.restored.len(), 1, "{report:?}");
    assert_eq!(fs::read(deployed.join("SKILL.md")).unwrap(), b"old");
    assert!(!new_dir.exists());
}

#[test]
fn a_committed_journal_only_needs_cleanup() {
    let fx = fixture("crash-committed");
    fs::create_dir_all(&fx.journal_dir).unwrap();
    let journal = serde_json::to_string(&serde_json::json!({
        "phase": "committed",
        "operationId": "op-1",
    }))
    .unwrap();
    fs::write(
        fx.journal_dir.join(JOURNAL_FILE_NAME),
        format!("{journal}\n"),
    )
    .unwrap();

    let report = recover_pending(&FsIo, &fx.journal_dir).unwrap();
    assert!(report.completed_cleanly);
    assert!(!fx.journal_dir.exists());
}

#[test]
fn recovery_leaves_everything_in_place_when_the_target_moved_on() {
    let fx = fixture("crash-conflict");
    let target = fx._dir.path().join("live").join("config.toml");
    fs::create_dir_all(&fx._dir.path().join("live")).unwrap();
    // The transaction wrote something, but a later writer changed it.
    fs::write(&target, "someone-else = true\n").unwrap();
    let journal = serde_json::to_string(&serde_json::json!({
        "phase": "stepCompleted",
        "target": 0,
        "index": 0,
        "detail": {
            "kind": "documentWritten",
            "path": target.to_string_lossy(),
            "syntax": "toml",
            "backupReference": null,
            "writtenHash": "0".repeat(64),
            "originalHash": null,
        },
    }))
    .unwrap();
    fs::create_dir_all(&fx.journal_dir).unwrap();
    fs::write(
        fx.journal_dir.join(JOURNAL_FILE_NAME),
        format!("{journal}\n"),
    )
    .unwrap();

    let report = recover_pending(&FsIo, &fx.journal_dir).unwrap();
    assert_eq!(report.failed.len(), 1);
    assert!(report.failed[0].contains("停止恢复"));
    // The conflicting journal stays for manual attention.
    assert!(fx.journal_dir.exists());
}

/// F03: an in-flight document replacement (started, not completed) is
/// undone from the `StepStarted` undo record by crash recovery.
#[test]
fn crash_between_replace_and_completion_undoes_from_step_started() {
    let fx = fixture("crash-in-flight");
    let target = fx._dir.path().join("live").join("config.toml");
    fs::create_dir_all(&fx._dir.path().join("live")).unwrap();
    fs::write(&target, "written = true\n").unwrap();
    // The transaction replaced the file; only the StepStarted record with
    // its undo info hit the journal.
    let journal = serde_json::to_string(&serde_json::json!({
        "phase": "stepStarted",
        "target": 0,
        "index": 0,
        "kind": "document",
        "path": target.to_string_lossy(),
        "undo": {
            "kind": "documentWritten",
            "path": target.to_string_lossy(),
            "syntax": "toml",
            "backupReference": null,
            "writtenHash": sha(&target),
            "originalHash": null,
        },
    }))
    .unwrap();
    fs::create_dir_all(&fx.journal_dir).unwrap();
    fs::write(
        fx.journal_dir.join(JOURNAL_FILE_NAME),
        format!("{journal}\n"),
    )
    .unwrap();

    let report = recover_pending(&FsIo, &fx.journal_dir).unwrap();
    assert!(report.failed.is_empty(), "{report:?}");
    assert!(!target.exists(), "the replaced file is removed again");
    assert!(!fx.journal_dir.exists());
}

/// F03: journal_lock_targets rebuilds the executor's sorted lock list from
/// a real executor journal.
#[test]
fn lock_targets_are_rebuilt_from_the_journal() {
    let fx = fixture("lock-targets");
    let first = fx._dir.path().join("live").join("config.toml");
    let second = fx._dir.path().join("live").join("settings.json");
    fs::create_dir_all(first.parent().unwrap()).unwrap();
    fs::write(&first, "model = \"gpt-5.2\"\n").unwrap();
    fs::write(&second, "{}\n").unwrap();
    let operation_plan = two_target_plan(&first, &second, &fx.backup_dir);
    let prepared = JournalEntry::Prepared {
        operation_id: "op-1".to_string(),
        created_at: "2026-09-06T00:00:00Z".to_string(),
        plan: serde_json::to_value(&operation_plan).unwrap(),
    };
    fs::write(
        fx.journal_dir.join(JOURNAL_FILE_NAME),
        format!("{}\n", serde_json::to_string(&prepared).unwrap()),
    )
    .unwrap();

    let targets = asb_switch::extensions::recovery::journal_lock_targets(&FsIo, &fx.journal_dir);
    assert_eq!(targets.len(), 2);
    assert!(targets.contains(&first));
    assert!(targets.contains(&second));
}

#[test]
fn lock_targets_fall_back_to_completed_steps_when_prepared_plan_is_damaged() {
    let fx = fixture("lock-targets-fallback");
    let target = fx._dir.path().join("live").join("config.toml");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "written = true\n").unwrap();
    let entry = JournalEntry::StepCompleted {
        target: 0,
        index: 0,
        detail: asb_core::extensions::contracts::AppliedStep::DocumentWritten {
            path: target.to_string_lossy().to_string(),
            syntax: DocumentSyntax::Toml,
            backup_reference: None,
            written_hash: sha(&target).unwrap(),
            original_hash: None,
        },
    };
    let prepared = JournalEntry::Prepared {
        operation_id: "op-1".to_string(),
        created_at: "2026-09-06T00:00:00Z".to_string(),
        plan: serde_json::json!({"damaged": true}),
    };
    fs::write(
        fx.journal_dir.join(JOURNAL_FILE_NAME),
        format!(
            "{}\n{}\n",
            serde_json::to_string(&prepared).unwrap(),
            serde_json::to_string(&entry).unwrap()
        ),
    )
    .unwrap();

    let targets = asb_switch::extensions::recovery::journal_lock_targets(&FsIo, &fx.journal_dir);
    assert_eq!(targets, vec![target]);
}
