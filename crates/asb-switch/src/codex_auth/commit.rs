//! Private Codex auth-file writes and snapshot-aware paired rollback.
use crate::{RecoveryOutcome, SwitchError, SwitchIo};
use asb_core::BackupRecord;
use std::path::Path;

pub(crate) fn commit_auth<Io: SwitchIo>(
    io: &Io,
    change: &crate::codex_auth::AuthChange,
) -> Result<(), SwitchError> {
    crate::codex_auth::verify_snapshot(io, change)?;
    let name = change
        .target
        .file_name()
        .expect("auth file name")
        .to_string_lossy();
    let temp = change
        .target
        .with_file_name(format!("{name}.{}.asb-auth-tmp", std::process::id()));
    io.write_new_file(&temp, &change.after)
        .map_err(|error| SwitchError::CommitFailed {
            stage: "auth-temp-write",
            message: error.to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        })?;
    if let Err(error) = io.set_mode(&temp, 0o600).and_then(|_| io.sync_file(&temp)) {
        let _ = io.remove(&temp);
        return Err(super::commit_error("auth-temp-sync", error.to_string()));
    }
    let read_back = io.read_file(&temp);
    if !matches!(read_back, Ok(ref text) if text == &change.after) {
        let _ = io.remove(&temp);
        return Err(SwitchError::CommitFailed {
            stage: "auth-temp-verify",
            message: "认证临时文件回读内容不匹配".into(),
            recovery: RecoveryOutcome::NotNeeded,
        });
    }
    if let Err(error) = super::verify_snapshot(io, change) {
        let _ = io.remove(&temp);
        return Err(error);
    }
    io.rename_replace(&temp, &change.target).map_err(|error| {
        let _ = io.remove(&temp);
        SwitchError::CommitFailed {
            stage: "auth-atomic-replace",
            message: error.to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        }
    })?;
    io.sync_dir(change.target.parent().expect("auth parent"))
        .map_err(|error| super::commit_error("auth-directory-sync", error.to_string()))?;
    match io.read_file(&change.target) {
        Ok(text) if text == change.after => Ok(()),
        _ => Err(SwitchError::CommitFailed {
            stage: "auth-post-verify",
            message: "认证文件替换后校验失败".into(),
            recovery: RecoveryOutcome::NotNeeded,
        }),
    }
}

pub(crate) fn restore_pair<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    config_backup: &BackupRecord,
    rendered: &str,
    auth: Option<(&crate::codex_auth::AuthChange, &BackupRecord)>,
) -> RecoveryOutcome {
    let config =
        crate::restore::restore_backup_if_unchanged(io, target, config_backup, rendered, true);
    let auth = match auth {
        Some((change, backup)) => crate::restore::restore_backup_if_unchanged(
            io,
            &change.target,
            backup,
            &change.after,
            true,
        ),
        None => RecoveryOutcome::NotNeeded,
    };
    match (config, auth) {
        (failure @ RecoveryOutcome::RestoreFailed { .. }, _)
        | (_, failure @ RecoveryOutcome::RestoreFailed { .. }) => failure,
        _ => RecoveryOutcome::Restored {
            backup: config_backup.clone(),
        },
    }
}
