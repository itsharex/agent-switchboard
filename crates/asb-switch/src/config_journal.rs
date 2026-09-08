//! Durable intent for a single client file; account credentials never enter this journal.
use crate::{io::SwitchIo, RecoveryOutcome, SwitchError};
use asb_core::AppKind;
use asb_core::BackupRecord;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingConfigWrite {
    pub version: u8,
    pub app: AppKind,
    pub profile_id: Option<String>,
    pub backup: BackupRecord,
    pub after_hash: String,
    pub after_existed: bool,
}

pub fn config_journal_path(directory: &Path, app: AppKind) -> PathBuf {
    directory.join(match app {
        AppKind::Codex => "pending-codex.json",
        AppKind::Claude => "pending-claude.json",
    })
}

pub fn pending_config_write<Io: SwitchIo>(
    io: &Io,
    directory: &Path,
    app: AppKind,
) -> Result<Option<PendingConfigWrite>, SwitchError> {
    let path = config_journal_path(directory, app);
    let text = match io.read_file(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(recovery_error("无法读取未完成配置事务", &path)),
    };
    let pending: PendingConfigWrite =
        serde_json::from_str(&text).map_err(|_| recovery_error("未完成配置事务格式无效", &path))?;
    if pending.version != 1
        || pending.app != app
        || pending.backup.app != app
        || Path::new(&pending.backup.backup_path).parent() != Some(directory)
        || pending.backup.linked_backup_id.is_some()
    {
        return Err(recovery_error("未完成配置事务身份不匹配", &path));
    }
    Ok(Some(pending))
}

pub(crate) fn require_clear<Io: SwitchIo>(
    io: &Io,
    directory: &Path,
    app: AppKind,
) -> Result<(), SwitchError> {
    if pending_config_write(io, directory, app)?.is_some() {
        return Err(recovery_error(
            "请先恢复未完成配置事务，再执行新的写入",
            &config_journal_path(directory, app),
        ));
    }
    Ok(())
}

pub(crate) fn track<Io: SwitchIo, T>(
    io: &Io,
    pending: PendingConfigWrite,
    execute: impl FnOnce() -> Result<T, SwitchError>,
) -> Result<T, SwitchError> {
    let directory = Path::new(&pending.backup.backup_path)
        .parent()
        .expect("backup directory");
    require_clear(io, directory, pending.app)?;
    let path = config_journal_path(directory, pending.app);
    let text = serde_json::to_string(&pending).expect("journal serializes");
    io.sync_file(Path::new(&pending.backup.backup_path))
        .map_err(|_| recovery_error("配置事务备份无法持久化", &path))?;
    io.write_new_file(&path, &text)
        .and_then(|_| io.sync_file(&path))
        .and_then(|_| io.sync_dir(directory))
        .map_err(|_| recovery_error("无法持久化配置事务；尚未写入配置", &path))?;
    let result = match execute() {
        Ok(result) => result,
        Err(failure) => {
            if matches!(
                &failure,
                SwitchError::ExternalChange { .. }
                    | SwitchError::CommitFailed {
                        recovery: RecoveryOutcome::NotNeeded,
                        ..
                    }
            ) {
                io.remove(&path)
                    .map_err(|_| recovery_error("未写入配置，但事务清理失败", &path))?;
            }
            return Err(failure);
        }
    };
    let target = Path::new(&pending.backup.target_path);
    if pending.after_existed {
        io.sync_file(target)
            .map_err(|_| recovery_error("配置提交尚未持久化，保留恢复记录", &path))?;
    }
    io.sync_dir(target.parent().expect("target directory"))
        .map_err(|_| recovery_error("配置目录提交尚未持久化，保留恢复记录", &path))?;
    io.remove(&path)
        .and_then(|_| io.sync_dir(directory))
        .map_err(|_| recovery_error("配置已提交，但事务清理未完成", &path))?;
    Ok(result)
}

/// A recovery never guesses when a user edited the file after the transaction.
pub fn finish_config_recovery<Io: SwitchIo>(
    io: &Io,
    directory: &Path,
    pending: &PendingConfigWrite,
    expected_hash: &str,
    expected_existed: bool,
    commit: impl FnOnce() -> Result<(), String>,
) -> Result<(), SwitchError> {
    let target = Path::new(&pending.backup.target_path);
    let path = config_journal_path(directory, pending.app);
    match crate::lockfile::acquire(io, target, crate::executor::PROCESS_NAME) {
        crate::AcquireOutcome::Acquired => {}
        crate::AcquireOutcome::Busy(status) => return Err(SwitchError::BlockedByLock { status }),
    }
    let result = (|| {
        let (current, existed) = crate::executor::read_current_or_empty(io, target, pending.app)
            .map_err(|_| recovery_error("无法读取恢复目标", &path))?;
        if existed != expected_existed || crate::sha256_hex(&current) != expected_hash {
            return Err(recovery_error(
                "配置已发生额外变化，请从事务备份显式恢复",
                &path,
            ));
        }
        commit().map_err(|message| recovery_error(&message, &path))?;
        sync_target(io, target, expected_existed, &path)?;
        io.remove(&path)
            .and_then(|_| io.sync_dir(directory))
            .map_err(|_| recovery_error("恢复已完成，但事务清理失败", &path))
    })();
    let released = crate::lockfile::release(io, target);
    match (result, released) {
        (result, Ok(())) => result,
        (Err(prior), Err(message)) => Err(SwitchError::LockReleaseFailed {
            message,
            prior: Box::new(prior),
        }),
        (Ok(()), Err(message)) => Err(recovery_error(&message, &path)),
    }
}

/// Explicit compensation for the exact pending backup, using the executor's existing verified rollback.
pub fn rollback_pending_config<Io: SwitchIo>(
    io: &Io,
    directory: &Path,
    pending: &PendingConfigWrite,
    commit: impl FnOnce() -> Result<(), String>,
) -> Result<crate::RestoreOutcome, SwitchError> {
    let target = Path::new(&pending.backup.target_path);
    match crate::lockfile::acquire(io, target, crate::executor::PROCESS_NAME) {
        crate::AcquireOutcome::Acquired => {}
        crate::AcquireOutcome::Busy(status) => return Err(SwitchError::BlockedByLock { status }),
    }
    let journal_path = config_journal_path(directory, pending.app);
    let result = (|| {
        let (current, existed) = crate::executor::read_current_or_empty(io, target, pending.app)
            .map_err(|_| recovery_error("无法读取显式补偿前的配置", &journal_path))?;
        let pre_restore_backup = crate::restore::snapshot_before_restore(
            io,
            &pending.backup,
            target,
            &current,
            existed,
        )?;
        let restored = crate::restore::restore_backup_content(io, target, &pending.backup);
        if !matches!(restored, RecoveryOutcome::Restored { .. }) {
            return Err(SwitchError::CommitFailed {
                stage: "transaction-recovery",
                message: "事务配置补偿未完成".into(),
                recovery: restored,
            });
        }
        commit().map_err(|message| recovery_error(&message, &journal_path))?;
        sync_target(io, target, pending.backup.target_existed, &journal_path)?;
        io.remove(&journal_path)
            .and_then(|_| io.sync_dir(directory))
            .map_err(|_| recovery_error("事务补偿清理失败", &journal_path))?;
        Ok(crate::RestoreOutcome {
            pre_restore_backup,
            restored_hash: pending.backup.content_hash.clone(),
            warnings: vec![],
        })
    })();
    match crate::lockfile::release(io, target) {
        Ok(()) => result,
        Err(message) => Err(recovery_error(&message, &journal_path)),
    }
}

fn sync_target<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    existed: bool,
    journal: &Path,
) -> Result<(), SwitchError> {
    if existed {
        io.sync_file(target)
            .map_err(|_| recovery_error("恢复配置尚未持久化", journal))?;
    }
    io.sync_dir(target.parent().expect("target directory"))
        .map_err(|_| recovery_error("恢复目录尚未持久化", journal))
}

fn recovery_error(message: &str, path: &Path) -> SwitchError {
    SwitchError::PlanRejected {
        message: format!("{message}；恢复记录：{}", path.display()),
        line: None,
    }
}
