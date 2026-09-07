//! The locked single-file transactions: provider projection and client-file
//! restore share one lock, backup, and rollback discipline.

use asb_core::{adapter, AppKind, BackupRecord, LockStatus, SwitchPlan, SwitchPreview};
use std::path::Path;

use super::preview::{plan_rejected, read_current_or_empty};
use super::{
    sha256_hex, timestamp_name, write_backup_metadata, RecoveryOutcome, RestoreOutcome,
    SwitchError, SwitchOutcome, SwitchRequest, PROCESS_NAME,
};
use crate::io::SwitchIo;
use crate::lockfile::{self, AcquireOutcome};
use crate::restore::{restore_backup_content, restore_locked};

/// Executes one provider projection transactionally. A successful client
/// write is not complete until `commit` records the same fact in application
/// state; if that commit fails, this function restores the just-created
/// backup before releasing the lock. There is deliberately no no-commit
/// execution entry point.
pub fn execute<Io: SwitchIo, Commit>(
    io: &Io,
    req: &SwitchRequest,
    commit: Commit,
) -> Result<SwitchOutcome, SwitchError>
where
    Commit: FnOnce(&SwitchOutcome) -> Result<(), String>,
{
    if let Some(parent) = req.target.parent() {
        io.ensure_dir(parent)
            .map_err(|error| SwitchError::CommitFailed {
                stage: "target-dir",
                message: error.to_string(),
                recovery: RecoveryOutcome::NotNeeded,
            })?;
    }
    match lockfile::acquire(io, req.target, PROCESS_NAME) {
        AcquireOutcome::Acquired => {}
        AcquireOutcome::Busy(status) => return Err(SwitchError::BlockedByLock { status }),
    }
    execute_locked(
        io,
        req.target,
        req.backup_dir,
        req.expected_hash,
        req.expected_rendered_hash,
        req.plan,
        commit,
    )
}

/// Restores one recorded client configuration backup through the same locked
/// transaction boundary as provider projection. The caller must commit the
/// corresponding application write record before this function releases the
/// lock; a commit failure restores the pre-restore snapshot.
pub fn restore<Io: SwitchIo, Commit>(
    io: &Io,
    backup: &BackupRecord,
    target: &Path,
    commit: Commit,
) -> Result<RestoreOutcome, SwitchError>
where
    Commit: FnOnce(&RestoreOutcome) -> Result<(), String>,
{
    if let Some(parent) = target.parent() {
        io.ensure_dir(parent)
            .map_err(|error| SwitchError::CommitFailed {
                stage: "target-dir",
                message: error.to_string(),
                recovery: RecoveryOutcome::NotNeeded,
            })?;
    }
    match lockfile::acquire(io, target, PROCESS_NAME) {
        AcquireOutcome::Busy(status) => Err(SwitchError::BlockedByLock { status }),
        AcquireOutcome::Acquired => {
            let result = restore_locked(io, backup, target, commit);
            match lockfile::release(io, target) {
                Ok(()) => result,
                Err(message) => match result {
                    Ok(mut outcome) => {
                        outcome
                            .warnings
                            .push(format!("配置已恢复，但写入锁释放失败：{message}"));
                        Ok(outcome)
                    }
                    Err(prior) => Err(SwitchError::LockReleaseFailed {
                        message,
                        prior: Box::new(prior),
                    }),
                },
            }
        }
    }
}

/// Reads the live file and refuses to continue when its hash no longer
/// matches the previewed state. Returns the text, whether the target
/// existed, and the verified hash.
pub(crate) fn read_unchanged_current<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    app: AppKind,
    expected_hash: &str,
) -> Result<(String, bool, String), SwitchError> {
    let (current, target_existed) =
        read_current_or_empty(io, target, app).map_err(|e| SwitchError::ReadCurrent {
            message: e.to_string(),
        })?;
    let found_hash = sha256_hex(&current);
    if found_hash != expected_hash {
        return Err(SwitchError::ExternalChange {
            expected_hash: expected_hash.to_string(),
            found_hash,
        });
    }
    Ok((current, target_existed, found_hash))
}

/// Re-reads the target at the last recoverable point before a mutation. The
/// comparison includes existence because an empty document and a missing file
/// have the same content hash but different restoration semantics.
pub(crate) fn verify_live_snapshot<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    app: AppKind,
    expected_content: &str,
    expected_existed: bool,
) -> Result<(), SwitchError> {
    let (found_content, found_existed) =
        read_current_or_empty(io, target, app).map_err(|error| SwitchError::ReadCurrent {
            message: error.to_string(),
        })?;
    if found_content == expected_content && found_existed == expected_existed {
        return Ok(());
    }
    Err(SwitchError::ExternalChange {
        expected_hash: sha256_hex(expected_content),
        found_hash: sha256_hex(&found_content),
    })
}

/// Produces the preview and the exact private candidate rendering, refusing
/// a plan whose rendering changed after the user saw the preview.
fn plan_candidate(
    plan: &SwitchPlan,
    current: &str,
    backup_dir: &str,
    expected_rendered_hash: &str,
) -> Result<(SwitchPreview, String), SwitchError> {
    let preview = adapter::preview(current, plan, backup_dir).map_err(plan_rejected)?;
    let rendered = adapter::render(current, plan).map_err(plan_rejected)?;
    if sha256_hex(&rendered) != expected_rendered_hash {
        return Err(SwitchError::PlanChanged);
    }
    Ok((preview, rendered))
}

/// Snapshots the current content as the pre-write backup, sidecar metadata
/// included.
pub(crate) fn back_up_current<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    backup_dir: &Path,
    current: &str,
    found_hash: &str,
    target_existed: bool,
    app: AppKind,
    reason: &str,
) -> Result<BackupRecord, SwitchError> {
    verify_live_snapshot(io, target, app, current, target_existed)?;
    io.ensure_dir(backup_dir)
        .map_err(|e| SwitchError::CommitFailed {
            stage: "backup-dir",
            message: e.to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        })?;
    let ts = timestamp_name(io);
    let file_name = target
        .file_name()
        .expect("target has a file name")
        .to_string_lossy()
        .to_string();
    let backup_path = backup_dir.join(format!("{file_name}.{ts}.bak"));
    let created_at = io.now_rfc3339();
    io.write_new_file(&backup_path, current)
        .map_err(|e| SwitchError::CommitFailed {
            stage: "backup",
            message: e.to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        })?;
    let backup_text = io
        .read_file(&backup_path)
        .map_err(|error| SwitchError::CommitFailed {
            stage: "backup-verify",
            message: error.to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        })?;
    if backup_text != current || adapter::validate_syntax(app, &backup_text).is_err() {
        return Err(SwitchError::CommitFailed {
            stage: "backup-verify",
            message: "备份回读内容或语法不匹配".to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        });
    }
    let backup = BackupRecord {
        id: format!("{}-{ts}", &found_hash[..12.min(found_hash.len())]),
        app,
        target_path: target.to_string_lossy().to_string(),
        backup_path: backup_path.to_string_lossy().to_string(),
        created_at,
        content_hash: found_hash.to_string(),
        target_existed,
        linked_backup_id: None,
        reason: reason.to_string(),
    };
    write_backup_metadata(io, &backup, "backup-meta")?;
    Ok(backup)
}

/// Writes the rendered candidate: temporary file → syntax validation →
/// atomic replacement → post-write verification. A failed stage after the
/// replacement restores the just-created backup.
pub(crate) fn commit_rendered<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    app: AppKind,
    rendered: &str,
    backup: &BackupRecord,
    expected_current: &str,
) -> Result<(), SwitchError> {
    let file_name = target
        .file_name()
        .expect("target has a file name")
        .to_string_lossy()
        .to_string();
    let temp_path = target.with_file_name(format!("{}.{}.asb-tmp", file_name, std::process::id()));
    io.write_new_file(&temp_path, rendered)
        .map_err(|e| SwitchError::CommitFailed {
            stage: "temp-write",
            message: e.to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        })?;

    let temp_text = match io.read_file(&temp_path) {
        Ok(text) if text == rendered => text,
        Ok(_) => {
            let _ = io.remove(&temp_path);
            return Err(SwitchError::CommitFailed {
                stage: "temp-verify",
                message: "临时文件回读内容不匹配".to_string(),
                recovery: RecoveryOutcome::NotNeeded,
            });
        }
        Err(error) => {
            let _ = io.remove(&temp_path);
            return Err(SwitchError::CommitFailed {
                stage: "temp-verify",
                message: error.to_string(),
                recovery: RecoveryOutcome::NotNeeded,
            });
        }
    };
    if let Err(e) = adapter::validate_syntax(app, &temp_text) {
        let _ = io.remove(&temp_path);
        return Err(SwitchError::CommitFailed {
            stage: "temp-validate",
            message: format!("临时文件校验失败: {e}"),
            recovery: RecoveryOutcome::NotNeeded,
        });
    }

    // The temp is now known good. Check the live target once more before the
    // irreversible replacement so a host edit made while creating the backup
    // cannot be silently overwritten.
    if let Err(error) =
        verify_live_snapshot(io, target, app, expected_current, backup.target_existed)
    {
        let _ = io.remove(&temp_path);
        return Err(error);
    }

    if let Err(e) = io.rename_replace(&temp_path, target) {
        let _ = io.remove(&temp_path);
        return Err(SwitchError::CommitFailed {
            stage: "atomic-replace",
            message: e.to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        });
    }

    // Post-write verification: the live file must equal the rendered
    // candidate and parse cleanly. Otherwise restore the backup.
    let verified = match io.read_file(target) {
        Ok(text) => text == rendered && adapter::validate_syntax(app, &text).is_ok(),
        Err(_) => false,
    };
    if !verified {
        let recovery = restore_backup_content(io, target, backup);
        return Err(SwitchError::CommitFailed {
            stage: "post-verify",
            message: "替换后校验失败".to_string(),
            recovery,
        });
    }
    Ok(())
}

/// The locked body of one transaction: verify → plan → re-verify → backup →
/// commit. Every exit path releases the lock through `finish`.
fn execute_locked<Io: SwitchIo, Commit>(
    io: &Io,
    target: &Path,
    backup_dir: &Path,
    expected_hash: &str,
    expected_rendered_hash: &str,
    plan: &SwitchPlan,
    commit: Commit,
) -> Result<SwitchOutcome, SwitchError>
where
    Commit: FnOnce(&SwitchOutcome) -> Result<(), String>,
{
    let finish = |result: Result<SwitchOutcome, SwitchError>| match lockfile::release(io, target) {
        Ok(()) => result,
        Err(message) => match result {
            Ok(mut outcome) => {
                outcome
                    .warnings
                    .push(format!("写入已完成，但无法释放写入锁：{message}"));
                Ok(outcome)
            }
            Err(prior) => Err(SwitchError::LockReleaseFailed {
                message,
                prior: Box::new(prior),
            }),
        },
    };

    let (initial_current, initial_target_existed, _) =
        match read_unchanged_current(io, target, plan.app(), expected_hash) {
            Ok(verified) => verified,
            Err(error) => return finish(Err(error)),
        };
    let backup_dir_label = backup_dir.to_string_lossy().to_string();
    let (current, target_existed, found_hash) =
        match read_unchanged_current(io, target, plan.app(), expected_hash) {
            Ok(verified) => verified,
            Err(error) => return finish(Err(error)),
        };
    if current != initial_current || target_existed != initial_target_existed {
        return finish(Err(SwitchError::ExternalChange {
            expected_hash: sha256_hex(&initial_current),
            found_hash,
        }));
    }
    let (preview, rendered) =
        match plan_candidate(plan, &current, &backup_dir_label, expected_rendered_hash) {
            Ok(candidate) => candidate,
            Err(error) => return finish(Err(error)),
        };
    let backup = match back_up_current(
        io,
        target,
        backup_dir,
        &current,
        &found_hash,
        target_existed,
        plan.app(),
        "provider-projection",
    ) {
        Ok(backup) => backup,
        Err(error) => return finish(Err(error)),
    };
    if let Err(error) = commit_rendered(io, target, plan.app(), &rendered, &backup, &current) {
        return finish(Err(error));
    }

    let outcome = SwitchOutcome {
        lock: LockStatus::Free,
        acquired_at: backup.created_at.clone(),
        changed: vec![target.to_string_lossy().to_string()],
        warnings: preview.warnings.clone(),
        backup,
        preview,
        recovery: RecoveryOutcome::NotNeeded,
        final_hash: sha256_hex(&rendered),
    };
    if let Err(message) = commit(&outcome) {
        let recovery = restore_backup_content(io, target, &outcome.backup);
        return finish(Err(SwitchError::CommitFailed {
            stage: "state-save",
            message,
            recovery,
        }));
    }

    finish(Ok(outcome))
}
