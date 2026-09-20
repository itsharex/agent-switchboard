//! A configuration restore and its linked Codex auth snapshot form one transaction.
use super::*;
struct Prepared {
    content: String,
    restored_hash: String,
    current: String,
    target_existed: bool,
    pre: BackupRecord,
    auth_backup: Option<BackupRecord>,
    auth_content: Option<String>,
    auth_current: Option<(String, bool)>,
    auth_pre: Option<BackupRecord>,
}
fn prepare<Io: SwitchIo>(
    io: &Io,
    backup: &BackupRecord,
    target: &Path,
    projected: Option<&str>,
) -> Result<Prepared, SwitchError> {
    let original = read_verified_restore_source(io, backup)?;
    let content = projected.unwrap_or(&original).to_string();
    adapter::validate_syntax(backup.app, &content).map_err(|error| SwitchError::PlanRejected {
        message: error.message,
        line: error.line,
    })?;
    validate_restore_contract(backup, &content)?;
    let auth_backup = linked_auth_backup(io, backup)?;
    let auth_content = auth_backup
        .as_ref()
        .map(|record| read_verified_auth_source(io, record))
        .transpose()?;
    let auth_current = auth_backup
        .as_ref()
        .map(|record| read_optional_file(io, Path::new(&record.target_path)))
        .transpose()?;
    if let (Some(content), Some((current, _))) = (&auth_content, &auth_current) {
        crate::codex_auth::validate_restore_generation(current, content)?;
    }
    let (current, target_existed) =
        read_current_or_empty(io, target, backup.app).map_err(|e| SwitchError::ReadCurrent {
            message: e.to_string(),
        })?;
    let pre = snapshot_before_restore(io, backup, target, &current, target_existed)?;
    let auth_pre = match (&auth_backup, &auth_current) {
        (Some(record), Some((current, existed))) => {
            let mut before = snapshot_before_restore(
                io,
                backup,
                Path::new(&record.target_path),
                current,
                *existed,
            )?;
            before.linked_backup_id = Some(pre.id.clone());
            before.reason = "codex-auth-projection".into();
            write_backup_metadata(io, &before, "restore-auth-backup-meta")?;
            Some(before)
        }
        _ => None,
    };
    Ok(Prepared {
        restored_hash: sha256_hex(&content),
        content,
        current,
        target_existed,
        pre,
        auth_backup,
        auth_content,
        auth_current,
        auth_pre,
    })
}
pub(crate) fn restore_locked<Io: SwitchIo, Commit>(
    io: &Io,
    backup: &BackupRecord,
    target: &Path,
    projected: Option<&str>,
    commit: Commit,
) -> Result<RestoreOutcome, SwitchError>
where
    Commit: FnOnce(&RestoreOutcome) -> Result<(), String>,
{
    let prepared = prepare(io, backup, target, projected)?;
    let pending = crate::PendingConfigWrite {
        version: 1,
        app: backup.app,
        profile_id: None,
        backup: prepared.pre.clone(),
        after_hash: prepared.restored_hash.clone(),
        after_existed: backup.target_existed,
        auth: prepared
            .auth_backup
            .as_ref()
            .zip(prepared.auth_pre.as_ref())
            .zip(prepared.auth_content.as_ref())
            .map(|((source, pre), content)| crate::PendingAuthWrite {
                backup: pre.clone(),
                after_hash: sha256_hex(content),
                after_existed: source.target_existed,
            }),
    };
    crate::config_journal::track(io, pending, || {
        if let Err(error) = replace_files(io, backup, target, &prepared) {
            return Err(SwitchError::CommitFailed {
                stage: "restore-write",
                message: error.to_string(),
                recovery: rollback(io, backup, target, &prepared),
            });
        }
        if !verified(io, backup, target, &prepared) {
            return Err(SwitchError::CommitFailed {
                stage: "restore-verify",
                message: "配置或认证恢复后校验失败".into(),
                recovery: rollback(io, backup, target, &prepared),
            });
        }
        let outcome = RestoreOutcome {
            pre_restore_backup: prepared.pre.clone(),
            restored_hash: prepared.restored_hash.clone(),
            warnings: vec![],
        };
        if let Err(message) = commit(&outcome) {
            return Err(SwitchError::CommitFailed {
                stage: "state-save",
                message,
                recovery: rollback(io, backup, target, &prepared),
            });
        }
        Ok(outcome)
    })
}
fn replace_files<Io: SwitchIo>(
    io: &Io,
    backup: &BackupRecord,
    target: &Path,
    p: &Prepared,
) -> Result<(), SwitchError> {
    if backup.target_existed {
        write_restore_candidate(
            io,
            target,
            backup.app,
            &p.content,
            &p.current,
            p.target_existed,
        )?;
    } else if p.target_existed {
        verify_live_snapshot(io, target, backup.app, &p.current, p.target_existed)?;
        io.remove(target).map_err(|e| SwitchError::CommitFailed {
            stage: "restore-replace",
            message: e.to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        })?;
    }
    if let (Some(source), Some(content), Some((current, existed))) =
        (&p.auth_backup, &p.auth_content, &p.auth_current)
    {
        write_auth_restore_candidate(
            io,
            Path::new(&source.target_path),
            content,
            current,
            *existed,
            source.target_existed,
        )?;
    }
    Ok(())
}
fn verified<Io: SwitchIo>(io: &Io, backup: &BackupRecord, target: &Path, p: &Prepared) -> bool {
    let config_ok = match io.read_file(target) {
        Ok(text) => {
            backup.target_existed
                && text == p.content
                && adapter::validate_syntax(backup.app, &text).is_ok()
        }
        Err(e) => e.kind() == ErrorKind::NotFound && !backup.target_existed,
    };
    let auth_ok = match (&p.auth_backup, &p.auth_content) {
        (Some(source), Some(content)) => match io.read_file(Path::new(&source.target_path)) {
            Ok(text) => source.target_existed && text == *content,
            Err(e) => e.kind() == ErrorKind::NotFound && !source.target_existed,
        },
        _ => true,
    };
    config_ok && auth_ok
}
fn rollback<Io: SwitchIo>(
    io: &Io,
    source: &BackupRecord,
    target: &Path,
    p: &Prepared,
) -> RecoveryOutcome {
    let config = restore_backup_if_unchanged(io, target, &p.pre, &p.content, source.target_existed);
    let auth = match (&p.auth_pre, &p.auth_content, &p.auth_backup) {
        (Some(pre), Some(content), Some(source)) => restore_backup_if_unchanged(
            io,
            Path::new(&pre.target_path),
            pre,
            content,
            source.target_existed,
        ),
        _ => RecoveryOutcome::NotNeeded,
    };
    match (config, auth) {
        (failure @ RecoveryOutcome::RestoreFailed { .. }, _)
        | (_, failure @ RecoveryOutcome::RestoreFailed { .. }) => failure,
        _ => RecoveryOutcome::Restored {
            backup: p.pre.clone(),
        },
    }
}
