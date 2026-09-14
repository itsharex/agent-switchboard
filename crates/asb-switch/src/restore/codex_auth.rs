//! Codex auth backup association and replacement, separate from config syntax.
use super::*;

pub(super) fn linked_auth_backup<Io: SwitchIo>(
    io: &Io,
    config_backup: &BackupRecord,
) -> Result<Option<BackupRecord>, SwitchError> {
    if config_backup.app != asb_core::AppKind::Codex {
        return Ok(None);
    }
    let directory = Path::new(&config_backup.backup_path)
        .parent()
        .expect("backup directory");
    let matches: Vec<_> = list_backups(io, directory)
        .into_iter()
        .filter(|record| {
            record.linked_backup_id.as_deref() == Some(config_backup.id.as_str())
                && Path::new(&record.target_path)
                    == Path::new(&config_backup.target_path).with_file_name("auth.json")
        })
        .collect();
    match matches.as_slice() {
        [] => Ok(None),
        [record] => Ok(Some(record.clone())),
        _ => Err(SwitchError::PlanRejected {
            message: "配置备份关联了多个认证备份".into(),
            line: None,
        }),
    }
}

pub(super) fn read_verified_auth_source<Io: SwitchIo>(
    io: &Io,
    backup: &BackupRecord,
) -> Result<String, SwitchError> {
    let content = io
        .read_file(Path::new(&backup.backup_path))
        .map_err(|error| SwitchError::CommitFailed {
            stage: "restore-backup-verify",
            message: format!("认证备份不可读: {error}"),
            recovery: RecoveryOutcome::NotNeeded,
        })?;
    if sha256_hex(&content) != backup.content_hash {
        return Err(SwitchError::ExternalChange {
            expected_hash: backup.content_hash.clone(),
            found_hash: sha256_hex(&content),
        });
    }
    Ok(content)
}

pub(super) fn read_optional_file<Io: SwitchIo>(
    io: &Io,
    target: &Path,
) -> Result<(String, bool), SwitchError> {
    match io.read_file(target) {
        Ok(content) => Ok((content, true)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok((String::new(), false)),
        Err(error) => Err(SwitchError::ReadCurrent {
            message: error.to_string(),
        }),
    }
}

pub(super) fn write_auth_restore_candidate<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    content: &str,
    current: &str,
    current_existed: bool,
    target_existed: bool,
) -> Result<(), SwitchError> {
    verify_live_snapshot(
        io,
        target,
        asb_core::AppKind::Codex,
        current,
        current_existed,
    )?;
    if !target_existed {
        if let Err(error) = io.remove(target) {
            if error.kind() != ErrorKind::NotFound {
                return Err(SwitchError::CommitFailed {
                    stage: "restore-replace",
                    message: error.to_string(),
                    recovery: RecoveryOutcome::NotNeeded,
                });
            }
        }
        return match io.read_file(target) {
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Ok(_) => Err(SwitchError::CommitFailed {
                stage: "restore-verify",
                message: "认证文件删除后仍可读取内容".into(),
                recovery: RecoveryOutcome::NotNeeded,
            }),
            Err(error) => Err(SwitchError::CommitFailed {
                stage: "restore-verify",
                message: error.to_string(),
                recovery: RecoveryOutcome::NotNeeded,
            }),
        };
    }
    let temp =
        stage_restore_file(io, target, content).map_err(|message| SwitchError::CommitFailed {
            stage: "auth-restore-stage",
            message,
            recovery: RecoveryOutcome::NotNeeded,
        })?;
    verify_live_snapshot(
        io,
        target,
        asb_core::AppKind::Codex,
        current,
        current_existed,
    )?;
    io.rename_replace(&temp, target)
        .and_then(|_| io.sync_dir(target.parent().expect("auth parent")))
        .map_err(|error| {
            let _ = io.remove(&temp);
            SwitchError::CommitFailed {
                stage: "restore-replace",
                message: error.to_string(),
                recovery: RecoveryOutcome::NotNeeded,
            }
        })
}
