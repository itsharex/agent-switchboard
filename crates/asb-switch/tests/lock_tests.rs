//! Lock classification semantics: held locks block, stale locks recover
//! explicitly, indeterminate locks refuse recovery.

mod common;

use asb_core::contracts::AppKind;
use asb_core::test_support::CODEX_TOML;
use asb_switch::io::FsIo;
use asb_switch::lockfile;
use asb_switch::{read_preview, SwitchError};
use common::{codex_plan, execute, setup};
use std::fs;

#[test]
fn held_lock_blocks_and_stale_lock_recovery_is_explicit() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let io = FsIo;
    let plan = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );
    let fp = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();

    // A live foreign holder: this test process itself.
    let lock_path = lockfile::lock_path_for(&target);
    let holder = serde_json::json!({
        "pid": std::process::id(),
        "process_name": "someone-else",
        "acquired_at": "2026-08-26T00:00:00Z",
    });
    fs::write(&lock_path, holder.to_string()).unwrap();

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
    assert!(matches!(
        err,
        SwitchError::BlockedByLock {
            status: asb_core::LockStatus::Held(_)
        }
    ));
    // The held lock was not deleted.
    assert!(lock_path.exists());

    // Use a PID outside the supported platform range instead of a just-reaped
    // one: the OS can immediately reuse an exited PID and make this assertion flaky.
    let dead_pid = u32::MAX;
    let stale = serde_json::json!({
        "pid": dead_pid,
        "process_name": "gone",
        "acquired_at": "2026-08-01T00:00:00Z",
    });
    fs::write(&lock_path, stale.to_string()).unwrap();
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
    assert!(matches!(
        err,
        SwitchError::BlockedByLock {
            status: asb_core::LockStatus::Stale(_)
        }
    ));
    assert!(
        lock_path.exists(),
        "stale lock must not be removed implicitly"
    );

    // Explicit recovery removes it and is reported.
    let entry = lockfile::recover_stale(&io, &target).unwrap();
    assert_eq!(entry.removed_holder_pid, Some(dead_pid));
    assert!(!lock_path.exists());

    // Now the switch succeeds.
    execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &fp.content_hash,
            expected_rendered_hash: &fp.rendered_hash,
        },
    )
    .unwrap();
    assert!(fs::read_to_string(&target)
        .unwrap()
        .contains("relay-b.internal"));
}

#[test]
fn indeterminate_lock_blocks_without_recovery() {
    let (_dir, target, _backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let io = FsIo;
    let lock_path = lockfile::lock_path_for(&target);
    fs::write(&lock_path, "not json at all").unwrap();

    let status = lockfile::probe_lock(&io, &target);
    assert!(matches!(status, asb_core::LockStatus::Indeterminate { .. }));
    // Recovery refuses anything that is not classified stale.
    let refused = lockfile::recover_stale(&io, &target).unwrap_err();
    assert!(matches!(
        refused,
        asb_core::LockStatus::Indeterminate { .. }
    ));
    assert!(lock_path.exists());
}

#[test]
fn missing_lock_is_reported_as_free() {
    let (_dir, target, _backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    assert!(matches!(
        lockfile::probe_lock(&FsIo, &target),
        asb_core::LockStatus::Free
    ));
}
