//! Client-file restore transactions and the backup listing they produce.

mod common;

use asb_core::contracts::AppKind;
use asb_core::test_support::CODEX_TOML;
use asb_switch::io::FsIo;
use asb_switch::lockfile;
use asb_switch::{read_preview, sha256_hex, RecoveryOutcome, SwitchError};
use common::{codex_plan, execute, restore, setup, FailingIo};
use std::fs;

#[test]
fn restore_restores_the_client_file_when_state_commit_fails() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let io = FsIo;
    let plan = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );
    let preview = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();
    let switched = execute(
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
    let switched_content = fs::read_to_string(&target).unwrap();
    let lock_path = lockfile::lock_path_for(&target);

    let error = asb_switch::restore(&io, &switched.backup, &target, |_| {
        assert!(
            lock_path.exists(),
            "state commit occurs before lock release"
        );
        Err("application state unavailable".to_string())
    })
    .unwrap_err();

    assert!(matches!(
        error,
        SwitchError::CommitFailed {
            stage: "state-save",
            recovery: RecoveryOutcome::Restored { .. },
            ..
        }
    ));
    assert_eq!(fs::read_to_string(&target).unwrap(), switched_content);
    assert!(!lock_path.exists());
}

#[test]
fn first_switch_creates_a_file_and_undo_restores_its_absence() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("live").join("config.toml");
    let backup_dir = dir.path().join("backups");
    let mut io = FailingIo::new();
    io.fixed_now = Some("2026-09-03T15:40:08.000Z");
    let plan = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );

    let fp = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();
    assert!(fp
        .preview
        .warnings
        .iter()
        .any(|warning| warning.contains("尚不存在")));
    let switched = execute(
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
    assert!(!switched.backup.target_existed);
    let switched_content = fs::read_to_string(&target).expect("created target");
    assert!(switched_content.contains("relay-b.internal"));

    let restored = restore(&io, &switched.backup, &target).expect("restore original absence");
    assert!(!target.exists());
    let records = asb_switch::list_backups(&io, &backup_dir);
    assert!(records
        .iter()
        .any(|record| record.id == restored.pre_restore_backup.id));

    let undone = restore(&io, &restored.pre_restore_backup, &target).expect("undo restore");
    assert_ne!(
        restored.pre_restore_backup.id, undone.pre_restore_backup.id,
        "fixed timestamps must still produce distinct backup records"
    );
    assert_ne!(
        restored.pre_restore_backup.backup_path, undone.pre_restore_backup.backup_path,
        "fixed timestamps must still produce distinct backup paths"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), switched_content);
}

#[test]
fn failed_restore_verification_recovers_the_pre_restore_snapshot() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let io = FsIo;
    let plan = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );
    let fp = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();
    let switched = execute(
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
    let switched_content = fs::read_to_string(&target).unwrap();

    let mut failing = FailingIo::new();
    failing.corrupt_after_rename = true;
    let err = restore(&failing, &switched.backup, &target).unwrap_err();

    match err {
        SwitchError::CommitFailed {
            stage, recovery, ..
        } => {
            assert_eq!(stage, "restore-verify");
            assert!(matches!(recovery, RecoveryOutcome::Restored { .. }));
        }
        other => panic!("expected restore verification failure, got {other:?}"),
    }
    assert_eq!(fs::read_to_string(&target).unwrap(), switched_content);
}

#[test]
fn restore_rejects_a_temporary_file_that_fails_its_readback_verification() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let io = FsIo;
    let plan = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );
    let preview = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();
    let switched = execute(
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
    let switched_content = fs::read_to_string(&target).unwrap();
    let failing = FailingIo {
        corrupt_temp_read: true,
        ..FailingIo::new()
    };

    let error = restore(&failing, &switched.backup, &target).unwrap_err();

    assert!(matches!(
        error,
        SwitchError::CommitFailed {
            stage: "restore-temp-verify",
            recovery: RecoveryOutcome::NotNeeded,
            ..
        }
    ));
    assert_eq!(fs::read_to_string(&target).unwrap(), switched_content);
    assert!(!lockfile::lock_path_for(&target).exists());
}

#[test]
fn restore_rejects_a_hash_matching_backup_with_invalid_syntax() {
    let (dir, target, _backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let invalid = "model = [\n";
    let backup_path = dir.path().join("invalid-config.toml.bak");
    fs::write(&backup_path, invalid).unwrap();
    let record = asb_core::BackupRecord {
        id: "invalid".to_string(),
        app: AppKind::Codex,
        target_path: target.to_string_lossy().to_string(),
        backup_path: backup_path.to_string_lossy().to_string(),
        created_at: "2026-09-01T00:00:00Z".to_string(),
        content_hash: sha256_hex(invalid),
        target_existed: true,
        linked_backup_id: None,
        reason: "test".to_string(),
    };

    let error = restore(&FsIo, &record, &target).unwrap_err();

    assert!(matches!(
        error,
        SwitchError::CommitFailed {
            stage: "restore-backup-verify",
            recovery: RecoveryOutcome::NotNeeded,
            ..
        }
    ));
    assert_eq!(fs::read_to_string(&target).unwrap(), CODEX_TOML);
    assert!(!lockfile::lock_path_for(&target).exists());
}

#[test]
fn list_backups_returns_records_with_hashes() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let io = FsIo;
    let plan_a = codex_plan("Relay A2", "https://a2.internal/v1", "m-a", "GATEWAY_A_KEY");
    let plan_b = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );
    let fp = read_preview(&io, &target, &plan_a, &backup_dir.to_string_lossy()).unwrap();
    execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan_a,
            backup_dir: &backup_dir,
            expected_hash: &fp.content_hash,
            expected_rendered_hash: &fp.rendered_hash,
        },
    )
    .unwrap();
    let fp2 = read_preview(&io, &target, &plan_b, &backup_dir.to_string_lossy()).unwrap();
    execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan_b,
            backup_dir: &backup_dir,
            expected_hash: &fp2.content_hash,
            expected_rendered_hash: &fp2.rendered_hash,
        },
    )
    .unwrap();

    let records = asb_switch::list_backups(&io, &backup_dir);
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|r| r.content_hash.len() == 64));
}

#[test]
fn backup_listing_ignores_metadata_that_points_outside_its_sidecar() {
    let (dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let io = FsIo;
    fs::create_dir_all(&backup_dir).unwrap();
    let outside = dir.path().join("outside.bak");
    fs::write(&outside, "not a backup").unwrap();
    let forged = asb_core::BackupRecord {
        id: "forged".to_string(),
        app: AppKind::Codex,
        target_path: target.to_string_lossy().to_string(),
        backup_path: outside.to_string_lossy().to_string(),
        created_at: "2026-08-26T00:00:00Z".to_string(),
        content_hash: sha256_hex("not a backup"),
        target_existed: true,
        linked_backup_id: None,
        reason: "switch".to_string(),
    };
    fs::write(
        backup_dir.join("forged.meta.json"),
        serde_json::to_string(&forged).unwrap(),
    )
    .unwrap();

    assert!(asb_switch::list_backups(&io, &backup_dir).is_empty());
}
