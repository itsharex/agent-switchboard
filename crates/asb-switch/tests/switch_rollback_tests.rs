//! Failure injection on the write path: every failed stage leaves the live
//! file either untouched or restored, and the lock state stays observable.

mod common;

use asb_core::contracts::AppKind;
use asb_core::test_support::CODEX_TOML;
use asb_switch::io::FsIo;
use asb_switch::lockfile;
use asb_switch::{read_preview, RecoveryOutcome, SwitchError};
use common::{codex_plan, execute, lock_path_exists, setup, FailingIo};
use std::fs;

#[test]
fn provider_projection_restores_the_client_file_when_state_commit_fails() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let io = FsIo;
    let plan = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );
    let preview = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();
    let lock_path = lockfile::lock_path_for(&target);

    let error = asb_switch::execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        |_| {
            assert!(
                lock_path.exists(),
                "state commit occurs before lock release"
            );
            Err("application state unavailable".to_string())
        },
    )
    .unwrap_err();

    assert!(matches!(
        error,
        SwitchError::CommitFailed {
            stage: "state-save",
            recovery: RecoveryOutcome::Restored { .. },
            ..
        }
    ));
    assert_eq!(fs::read_to_string(&target).unwrap(), CODEX_TOML);
    assert!(!lock_path.exists());
}

#[test]
fn executor_rejects_a_backup_that_fails_its_readback_verification() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let io = FailingIo {
        corrupt_backup_read: true,
        ..FailingIo::new()
    };
    let plan = codex_plan("Relay B", "https://relay-b.internal/v1", "gpt-5.2", "KEY");
    let preview = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();

    let error = execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
    )
    .unwrap_err();

    assert!(matches!(
        error,
        SwitchError::CommitFailed {
            stage: "backup-verify",
            recovery: RecoveryOutcome::NotNeeded,
            ..
        }
    ));
    assert_eq!(fs::read_to_string(&target).unwrap(), CODEX_TOML);
    assert!(!lockfile::lock_path_for(&target).exists());
}

#[test]
fn executor_rejects_a_temporary_file_that_fails_its_readback_verification() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let io = FailingIo {
        corrupt_temp_read: true,
        ..FailingIo::new()
    };
    let plan = codex_plan("Relay B", "https://relay-b.internal/v1", "gpt-5.2", "KEY");
    let preview = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();

    let error = execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
    )
    .unwrap_err();

    assert!(matches!(
        error,
        SwitchError::CommitFailed {
            stage: "temp-verify",
            recovery: RecoveryOutcome::NotNeeded,
            ..
        }
    ));
    assert_eq!(fs::read_to_string(&target).unwrap(), CODEX_TOML);
    assert!(!lockfile::lock_path_for(&target).exists());
}

#[test]
fn executor_preserves_a_host_edit_made_while_preparing_the_temporary_file() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let io = FailingIo {
        mutate_live_after_temp_write: true,
        ..FailingIo::new()
    };
    let plan = codex_plan("Relay B", "https://relay-b.internal/v1", "gpt-5.2", "KEY");
    let preview = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();

    let error = execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
    )
    .unwrap_err();

    assert!(matches!(error, SwitchError::ExternalChange { .. }));
    assert!(fs::read_to_string(&target)
        .unwrap()
        .contains("host_changed_during_switch = true"));
    assert!(!lockfile::lock_path_for(&target).exists());
}

#[test]
fn lock_release_failure_is_returned_as_an_observable_success_warning() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let io = FailingIo::new();
    io.fail_stage.set(Some("lock-release"));
    let plan = codex_plan("Relay B", "https://relay-b.internal/v1", "gpt-5.2", "KEY");
    let preview = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();

    let outcome = execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
    )
    .unwrap();

    assert!(outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("无法释放写入锁")));
    assert!(lockfile::lock_path_for(&target).exists());
}

#[test]
fn invalid_temporary_write_keeps_preceding_content() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let original = fs::read_to_string(&target).unwrap();
    let io = FailingIo::new();
    io.fail_stage.set(Some("temp-write"));

    let plan = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );
    let fp = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();
    let err = execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &fp.content_hash,
            expected_rendered_hash: &fp.rendered_hash,
        },
    )
    .unwrap_err();

    match &err {
        SwitchError::CommitFailed {
            stage, recovery, ..
        } => {
            assert_eq!(*stage, "temp-write");
            assert!(matches!(recovery, RecoveryOutcome::NotNeeded));
        }
        other => panic!("expected CommitFailed, got {other:?}"),
    }
    assert_eq!(fs::read_to_string(&target).unwrap(), original);
    // The lock was released even though the write failed.
    assert!(!lock_path_exists(&target));
}

#[test]
fn failed_post_write_verification_restores_the_backup() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let original = fs::read_to_string(&target).unwrap();
    let mut io = FailingIo::new();
    io.corrupt_after_rename = true;
    let plan = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );
    let fp = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();
    let err = execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &fp.content_hash,
            expected_rendered_hash: &fp.rendered_hash,
        },
    )
    .unwrap_err();

    match &err {
        SwitchError::CommitFailed {
            stage, recovery, ..
        } => {
            assert_eq!(*stage, "post-verify");
            assert!(
                matches!(recovery, RecoveryOutcome::Restored { .. }),
                "recovery: {recovery:?}"
            );
        }
        other => panic!("expected CommitFailed, got {other:?}"),
    }
    // The immediately preceding content is back, byte for byte.
    assert_eq!(fs::read_to_string(&target).unwrap(), original);
}

#[test]
fn external_edit_blocks_the_switch_with_a_diagnostic() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let io = FsIo;
    let plan = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );

    let fp = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();
    // The host edits the file between preview and switch.
    let edited = format!("{CODEX_TOML}\nthreads = 16\n");
    fs::write(&target, &edited).unwrap();

    let err = execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &fp.content_hash,
            expected_rendered_hash: &fp.rendered_hash,
        },
    )
    .unwrap_err();
    assert!(matches!(err, SwitchError::ExternalChange { .. }));
    assert_eq!(fs::read_to_string(&target).unwrap(), edited);
}

#[test]
fn changed_candidate_after_preview_is_blocked_without_writing() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let original = fs::read_to_string(&target).unwrap();
    let io = FsIo;
    let previewed = codex_plan(
        "Relay A",
        "https://relay-a.internal/v1",
        "gpt-5.1",
        "RELAY_A_KEY",
    );
    let changed = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "RELAY_B_KEY",
    );
    let fp = read_preview(&io, &target, &previewed, &backup_dir.to_string_lossy()).unwrap();

    let err = execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &changed,
            backup_dir: &backup_dir,
            expected_hash: &fp.content_hash,
            expected_rendered_hash: &fp.rendered_hash,
        },
    )
    .unwrap_err();

    assert!(matches!(err, SwitchError::PlanChanged));
    assert_eq!(fs::read_to_string(&target).unwrap(), original);
    assert!(!backup_dir.exists());
}
