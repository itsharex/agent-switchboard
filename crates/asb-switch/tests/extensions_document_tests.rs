//! Document-step transaction tests: writes, removals, staleness rejection,
//! batch rollback across targets, and lock blocking.

mod common;

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{DocumentSyntax, ExtensionTarget};
use asb_core::extensions::plan::PlanStep;
use asb_switch::{apply_extension_plan, ExtensionApplyRequest, ExtensionError, FsIo, SwitchIo};
use common::extensions::{apply, document_step, fixture, plan, sha, two_target_plan};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[test]
fn document_write_installs_a_server_and_preserves_host_content() {
    let fx = fixture("doc-write");
    let target = fx._dir.path().join("live").join("config.toml");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "# host comment\nmodel = \"gpt-5.2\"\n").unwrap();
    let expected = sha(&target);

    let mut plan = plan(
        &ExtensionTarget::App {
            client: AppKind::Codex,
        },
        vec![document_step(
            &target,
            expected,
            "# host comment\nmodel = \"gpt-5.2\"\n\n[mcp_servers.docs]\ncommand = \"npx\"\nenabled = true\n",
        )],
    );
    plan.operations[0].targets[0].steps[0] = PlanStep::DocumentWrite {
        client: AppKind::Codex,
        path: target.to_string_lossy().to_string(),
        expected_content_hash: sha(&target),
        expected_existed: true,
        rendered: "# host comment\nmodel = \"gpt-5.2\"\n\n[mcp_servers.docs]\ncommand = \"npx\"\nenabled = true\n".to_string(),
        syntax: DocumentSyntax::Toml,
        backup_dir: fx.backup_dir.to_string_lossy().to_string(),
    };

    let record = apply(&fx, &plan).unwrap();
    assert!(record.rollback.is_none());
    assert_eq!(
        record.resources[0].targets[0].outcome,
        asb_core::extensions::contracts::TargetOutcome::Applied
    );
    let written = fs::read_to_string(&target).unwrap();
    assert!(written.contains("[mcp_servers.docs]"));
    assert!(written.contains("# host comment"));
    // The journal is cleaned up after commit; the backup remains.
    assert!(!fx.journal_dir.exists());
    assert!(fs::read_dir(&fx.backup_dir).unwrap().next().is_some());
}

#[test]
fn stale_document_is_rejected_without_writing() {
    let fx = fixture("doc-stale");
    let target = fx._dir.path().join("live").join("config.toml");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "model = \"gpt-5.2\"\n").unwrap();
    let stale_hash = "0".repeat(64);

    let plan = plan(
        &ExtensionTarget::App {
            client: AppKind::Codex,
        },
        vec![PlanStep::DocumentWrite {
            client: AppKind::Codex,
            path: target.to_string_lossy().to_string(),
            expected_content_hash: Some(stale_hash),
            expected_existed: true,
            rendered: "model = \"changed\"\n".to_string(),
            syntax: DocumentSyntax::Toml,
            backup_dir: fx.backup_dir.to_string_lossy().to_string(),
        }],
    );

    match apply(&fx, &plan) {
        Err(ExtensionError::PlanStale { message }) => {
            assert!(message.contains("重新预览"));
        }
        other => panic!("expected PlanStale, got {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "model = \"gpt-5.2\"\n"
    );
}

#[test]
fn library_commit_failure_rolls_back_the_written_document() {
    let fx = fixture("doc-commit-fail");
    let target = fx._dir.path().join("live").join("config.toml");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "model = \"gpt-5.2\"\n").unwrap();
    let original = sha(&target);

    let plan = plan(
        &ExtensionTarget::App {
            client: AppKind::Codex,
        },
        vec![PlanStep::DocumentWrite {
            client: AppKind::Codex,
            path: target.to_string_lossy().to_string(),
            expected_content_hash: original.clone(),
            expected_existed: true,
            rendered: "model = \"gpt-5.2\"\n\n[mcp_servers.docs]\ncommand = \"npx\"\n".to_string(),
            syntax: DocumentSyntax::Toml,
            backup_dir: fx.backup_dir.to_string_lossy().to_string(),
        }],
    );

    let error = apply_extension_plan(
        &FsIo,
        &ExtensionApplyRequest {
            operation_id: "op-1",
            plan: &plan,
            journal_dir: &fx.journal_dir,
            library_commit: None,
        },
        |_record, _steps| Err("store unreachable".to_string().into()),
    )
    .unwrap_err();

    match &error {
        ExtensionError::Failed {
            message, rollback, ..
        } => {
            assert!(message.contains("扩展库提交失败"));
            assert!(rollback.restored.len() == 1, "rollback: {rollback:?}");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert_eq!(sha(&target).as_deref(), original.as_deref());
}

#[test]
fn library_commit_failure_restores_a_removed_document() {
    let fx = fixture("doc-remove-commit-fail");
    let target = fx._dir.path().join("live").join("config.toml");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "model = \"gpt-5.2\"\n").unwrap();
    let original = sha(&target).unwrap();

    let plan = plan(
        &ExtensionTarget::App {
            client: AppKind::Codex,
        },
        vec![PlanStep::DocumentRemove {
            client: AppKind::Codex,
            path: target.to_string_lossy().to_string(),
            expected_content_hash: original,
            syntax: DocumentSyntax::Toml,
            backup_dir: fx.backup_dir.to_string_lossy().to_string(),
        }],
    );

    let error = apply_extension_plan(
        &FsIo,
        &ExtensionApplyRequest {
            operation_id: "op-1",
            plan: &plan,
            journal_dir: &fx.journal_dir,
            library_commit: None,
        },
        |_record, _steps| Err("store unreachable".to_string().into()),
    )
    .unwrap_err();

    assert!(matches!(error, ExtensionError::Failed { .. }));
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "model = \"gpt-5.2\"\n"
    );
    assert!(!fx.journal_dir.exists());
}

/// Fails the Nth rename (`rename_replace` or plain `rename`) to simulate a
/// mid-batch crash between two targets.
struct FailNthRename {
    inner: FsIo,
    remaining: std::cell::Cell<u32>,
}

impl FailNthRename {
    fn after(count: u32) -> Self {
        Self {
            inner: FsIo,
            remaining: std::cell::Cell::new(count),
        }
    }

    fn consume(&self) -> bool {
        let left = self.remaining.get();
        if left == 0 {
            return true;
        }
        self.remaining.set(left - 1);
        false
    }
}

impl SwitchIo for FailNthRename {
    fn sync_file(&self, path: &Path) -> io::Result<()> {
        self.inner.sync_file(path)
    }
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        self.inner.sync_dir(path)
    }
    fn read_file(&self, path: &Path) -> io::Result<String> {
        self.inner.read_file(path)
    }
    fn write_new_file(&self, path: &Path, content: &str) -> io::Result<()> {
        self.inner.write_new_file(path, content)
    }
    fn write_file_replace(&self, path: &Path, content: &str) -> io::Result<()> {
        self.inner.write_file_replace(path, content)
    }
    fn rename_replace(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.consume() {
            return Err(io::Error::other("injected rename failure"));
        }
        self.inner.rename_replace(from, to)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.consume() {
            return Err(io::Error::other("injected rename failure"));
        }
        self.inner.rename(from, to)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        self.inner.remove(path)
    }
    fn ensure_dir(&self, path: &Path) -> io::Result<()> {
        self.inner.ensure_dir(path)
    }
    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        self.inner.list_dir(path)
    }
    fn now_rfc3339(&self) -> String {
        self.inner.now_rfc3339()
    }
    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.inner.read_bytes(path)
    }
    fn write_new_bytes(&self, path: &Path, content: &[u8]) -> io::Result<()> {
        self.inner.write_new_bytes(path, content)
    }
    fn set_mode(&self, path: &Path, mode: u32) -> io::Result<()> {
        self.inner.set_mode(path, mode)
    }
    fn write_bytes_replace(&self, path: &Path, content: &[u8]) -> io::Result<()> {
        self.inner.write_bytes_replace(path, content)
    }
    fn path_kind(&self, path: &Path) -> io::Result<asb_switch::io::PathKind> {
        self.inner.path_kind(path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_dir_all(path)
    }
}

#[test]
fn second_target_failure_rolls_back_the_first_target() {
    let fx = fixture("two-target");
    let first = fx._dir.path().join("live").join("config.toml");
    let second = fx._dir.path().join("live").join("settings.json");
    fs::create_dir_all(&fx._dir.path().join("live")).unwrap();
    fs::write(&first, "model = \"gpt-5.2\"\n").unwrap();
    fs::write(&second, "{\"model\": \"claude-sonnet-5\"}\n").unwrap();
    let first_original = sha(&first);
    let second_original = sha(&second);

    // The batch writes config.toml first; settings.json is written second.
    // The failing wrapper injects a rename failure into the SECOND target
    // while the first one already committed its write.
    let failing = FailNthRename::after(1);
    let plan = two_target_plan(&first, &second, &fx.backup_dir);

    let error = apply_extension_plan(
        &failing,
        &ExtensionApplyRequest {
            operation_id: "op-1",
            plan: &plan,
            journal_dir: &fx.journal_dir,
            library_commit: None,
        },
        |_record, _steps| Ok(()),
    )
    .unwrap_err();

    match &error {
        ExtensionError::Failed {
            record, rollback, ..
        } => {
            assert!(matches!(
                record.resources[0].targets[1].outcome,
                asb_core::extensions::contracts::TargetOutcome::Failed { .. }
            ));
            assert!(
                matches!(
                    record.resources[0].targets[0].outcome,
                    asb_core::extensions::contracts::TargetOutcome::Restored { .. }
                ),
                "first target should be restored: {:?}",
                record.resources[0].targets[0].outcome
            );
            assert!(!rollback.restored.is_empty());
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    // Both targets are back to their exact pre-batch content.
    assert_eq!(sha(&first).as_deref(), first_original.as_deref());
    assert_eq!(sha(&second).as_deref(), second_original.as_deref());
    // The locks acquired for the batch are released again.
    assert!(!first.with_file_name("config.toml.asb-lock").exists());
}

#[test]
fn an_occupied_lock_blocks_the_batch_without_writes() {
    let fx = fixture("locked");
    let target = fx._dir.path().join("live").join("config.toml");
    fs::create_dir_all(&fx._dir.path().join("live")).unwrap();
    fs::write(&target, "model = \"gpt-5.2\"\n").unwrap();
    let lock_path = target.with_file_name("config.toml.asb-lock");
    fs::write(
        &lock_path,
        r#"{"pid": 999999, "processName": "someone-else", "acquiredAt": "2026-09-06T00:00:00Z"}"#,
    )
    .unwrap();

    let plan = plan(
        &ExtensionTarget::App {
            client: AppKind::Codex,
        },
        vec![PlanStep::DocumentWrite {
            client: AppKind::Codex,
            path: target.to_string_lossy().to_string(),
            expected_content_hash: sha(&target),
            expected_existed: true,
            rendered: "model = \"changed\"\n".to_string(),
            syntax: DocumentSyntax::Toml,
            backup_dir: fx.backup_dir.to_string_lossy().to_string(),
        }],
    );

    match apply(&fx, &plan) {
        Err(ExtensionError::Blocked { message }) => {
            assert!(message.contains("正在被占用"));
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "model = \"gpt-5.2\"\n"
    );
    assert!(lock_path.exists(), "the foreign lock must not be removed");
}

/// F02: a failure in the *second* step of one target rolls back the first
/// step's already-written file.
#[test]
fn a_later_step_failure_rolls_back_earlier_writes_of_the_same_target() {
    let fx = fixture("same-target-rollback");
    let dir = fx._dir.path().join("live");
    fs::create_dir_all(&dir).unwrap();
    let first = dir.join("one.toml");
    let second = dir.join("two.toml");
    fs::write(&first, "value = 1\n").unwrap();
    fs::write(&second, "value = 1\n").unwrap();

    let mut target_plan = plan(
        &ExtensionTarget::App {
            client: AppKind::Codex,
        },
        vec![document_step(&first, sha(&first), "value = 2\n")],
    );
    // Second step carries a stale expectation: it fails after the first
    // step already wrote `value = 2`.
    target_plan.operations[0].targets[0]
        .steps
        .push(document_step(&second, Some("0".repeat(64)), "value = 2\n"));
    for step in &mut target_plan.operations[0].targets[0].steps {
        if let PlanStep::DocumentWrite { backup_dir, .. } = step {
            *backup_dir = fx.backup_dir.to_string_lossy().to_string();
        }
    }

    match apply(&fx, &target_plan) {
        Err(ExtensionError::Failed {
            rollback, record, ..
        }) => {
            assert_eq!(rollback.restored.len(), 1, "{rollback:?}");
            assert_eq!(record.resources[0].targets.len(), 1);
            assert!(matches!(
                record.resources[0].targets[0].outcome,
                asb_core::extensions::contracts::TargetOutcome::Failed { .. }
            ));
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert_eq!(fs::read_to_string(&first).unwrap(), "value = 1\n");
    assert_eq!(fs::read_to_string(&second).unwrap(), "value = 1\n");
}

/// A failed library commit reverses every client write and removes its
/// terminal journal. There is no stale transaction left for recovery to undo.
#[test]
fn failed_library_commit_rolls_back_and_cleans_its_terminal_journal() {
    let fx = fixture("journal-parseable");
    let target = fx._dir.path().join("live").join("config.toml");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "model = \"gpt-5.2\"\n").unwrap();
    let operation_plan = plan(
        &ExtensionTarget::App {
            client: AppKind::Codex,
        },
        vec![PlanStep::DocumentWrite {
            client: AppKind::Codex,
            path: target.to_string_lossy().to_string(),
            expected_content_hash: sha(&target),
            expected_existed: true,
            rendered: "model = \"gpt-5.2\"\n\n[mcp_servers.docs]\ncommand = \"npx\"\n".to_string(),
            syntax: DocumentSyntax::Toml,
            backup_dir: fx.backup_dir.to_string_lossy().to_string(),
        }],
    );
    let error = apply_extension_plan(
        &FsIo,
        &ExtensionApplyRequest {
            operation_id: "op-1",
            plan: &operation_plan,
            journal_dir: &fx.journal_dir,
            library_commit: Some(serde_json::json!({"marker": true})),
        },
        |_record, _steps| Err("store unavailable".to_string().into()),
    )
    .unwrap_err();
    assert!(matches!(error, ExtensionError::Failed { .. }));

    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "model = \"gpt-5.2\"\n"
    );
    assert!(
        !fx.journal_dir.exists(),
        "a completed rollback must not leave a pending transaction"
    );
}

/// F05: a plan past its expiry is rejected under lock, before any write.
#[test]
fn an_expired_plan_is_rejected_before_writing() {
    let fx = fixture("expired-plan");
    let target = fx._dir.path().join("live").join("config.toml");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "model = \"gpt-5.2\"\n").unwrap();
    let mut expired = plan(
        &ExtensionTarget::App {
            client: AppKind::Codex,
        },
        vec![document_step(
            &target,
            sha(&target),
            "model = \"changed\"\n",
        )],
    );
    expired.preconditions.expires_at = "2000-01-01T00:00:00+00:00".to_string();

    match apply(&fx, &expired) {
        Err(ExtensionError::PlanStale { message }) => {
            assert!(message.contains("过期"));
        }
        other => panic!("expected PlanStale, got {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "model = \"gpt-5.2\"\n"
    );
}
