//! Config-file backup and atomic replacement primitives used by the executor.
use super::*;

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
    // The snapshot is the recovery preimage, including malformed input to an
    // explicitly confirmed repair. Only the replacement must satisfy syntax.
    if backup_text != current {
        return Err(SwitchError::CommitFailed {
            stage: "backup-verify",
            message: "备份回读内容不匹配".to_string(),
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
    stage_config_file(io, &temp_path, app, rendered)?;
    // Recheck after preparing the candidate so concurrent host edits are preserved.
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
    if let Err(error) = io.sync_dir(target.parent().expect("config parent")) {
        return Err(SwitchError::CommitFailed {
            stage: "config-directory-sync",
            message: error.to_string(),
            recovery: restore_backup_content(io, target, backup),
        });
    }
    // Verify the actual replacement before committing application state.
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

fn stage_config_file<Io: SwitchIo>(
    io: &Io,
    temp_path: &Path,
    app: AppKind,
    rendered: &str,
) -> Result<(), SwitchError> {
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
    io.sync_file(temp_path).map_err(|error| {
        let _ = io.remove(temp_path);
        SwitchError::CommitFailed {
            stage: "config-temp-sync",
            message: error.to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        }
    })
}
