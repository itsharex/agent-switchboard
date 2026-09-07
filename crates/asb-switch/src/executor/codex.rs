//! Codex dual-file switching: `config.toml` and `auth.json` move inside one
//! transaction, with linked backups and credential-aware rollback.

use asb_core::{adapter, AppKind, BackupRecord, SwitchPlan};
use std::io::ErrorKind;
use std::path::Path;

use super::preview::{paired_hash, plan_rejected, read_auth_or_empty, read_current_or_empty};
use super::switch::{execute, restore};
use super::{
    recovery_label, sha256_hex, timestamp_name, write_backup_metadata, CodexSwitchRequest,
    RecoveryOutcome, RestoreOutcome, SwitchError, SwitchOutcome, SwitchRequest,
};
use crate::io::SwitchIo;
use crate::restore::restore_backup_content;

/// Executes a Codex switch that keeps `config.toml` and `auth.json` aligned.
/// The normal configuration transaction owns the lock and rollback. Its
/// state-commit phase performs the credential-cache mutation before recording
/// application state; any credential or state failure rolls both files back
/// to their immediately preceding verified backups.
pub fn execute_codex<Io: SwitchIo, Commit>(
    io: &Io,
    req: &CodexSwitchRequest,
    commit: Commit,
) -> Result<SwitchOutcome, SwitchError>
where
    Commit: FnOnce(&SwitchOutcome) -> Result<(), String>,
{
    if req.plan.app() != AppKind::Codex {
        return Err(SwitchError::PlanRejected {
            message: "Codex 双文件切换只接受 Codex 档案".to_string(),
            line: None,
        });
    }

    let (config_current, _) =
        read_current_or_empty(io, req.target, AppKind::Codex).map_err(|error| {
            SwitchError::ReadCurrent {
                message: error.to_string(),
            }
        })?;
    let (auth_current, _) =
        read_auth_or_empty(io, req.auth_target).map_err(|error| SwitchError::ReadCurrent {
            message: error.to_string(),
        })?;
    let config_hash = sha256_hex(&config_current);
    let auth_hash = sha256_hex(&auth_current);
    let found_hash = paired_hash(&config_hash, &auth_hash);
    if found_hash != req.expected_hash {
        return Err(SwitchError::ExternalChange {
            expected_hash: req.expected_hash.to_string(),
            found_hash,
        });
    }

    let config_rendered = adapter::render(&config_current, req.plan).map_err(plan_rejected)?;
    let auth_rendered =
        adapter::render_codex_auth(&auth_current, req.plan).map_err(plan_rejected)?;
    let config_rendered_hash = sha256_hex(&config_rendered);
    let auth_rendered_hash = sha256_hex(&auth_rendered);
    if paired_hash(&config_rendered_hash, &auth_rendered_hash) != req.expected_rendered_hash {
        return Err(SwitchError::PlanChanged);
    }

    let config_request = SwitchRequest {
        target: req.target,
        plan: req.plan,
        backup_dir: req.backup_dir,
        expected_hash: &config_hash,
        expected_rendered_hash: &config_rendered_hash,
    };
    let auth_changed = auth_rendered != auth_current;
    let mut credential_backup = None;
    let mut state_commit = Some(commit);
    let result = execute(io, &config_request, |outcome| {
        let backup = write_codex_auth(
            io,
            req.auth_target,
            req.backup_dir,
            req.plan,
            &auth_hash,
            &auth_rendered_hash,
            &outcome.backup.id,
        )?;
        credential_backup = Some(backup);

        let state_commit = state_commit.take().expect("one state commit");
        if let Err(message) = state_commit(outcome) {
            if auth_changed {
                let backup = credential_backup
                    .as_ref()
                    .expect("Codex authentication snapshot exists before state commit");
                let recovery = restore_backup_content(io, req.auth_target, backup);
                if !matches!(recovery, RecoveryOutcome::Restored { .. }) {
                    return Err(format!("{message}; Codex 登录缓存恢复失败"));
                }
            }
            return Err(message);
        }
        Ok(())
    });

    match result {
        Ok(mut outcome) => {
            if auth_changed {
                outcome
                    .changed
                    .push(req.auth_target.to_string_lossy().to_string());
            }
            Ok(outcome)
        }
        Err(error) => Err(error),
    }
}

/// Restores the configuration and credential snapshots produced by one Codex
/// switch. A credential snapshot is always linked to its configuration
/// snapshot, so a restored endpoint can never retain a key from another
/// relay.
pub fn restore_codex<Io: SwitchIo, Commit>(
    io: &Io,
    backup: &BackupRecord,
    auth_backup: &BackupRecord,
    target: &Path,
    auth_target: &Path,
    commit: Commit,
) -> Result<RestoreOutcome, SwitchError>
where
    Commit: FnOnce(&RestoreOutcome) -> Result<(), String>,
{
    if backup.app != AppKind::Codex
        || auth_backup.app != AppKind::Codex
        || auth_backup.linked_backup_id.as_deref() != Some(backup.id.as_str())
        || Path::new(&auth_backup.target_path) != auth_target
    {
        return Err(SwitchError::PlanRejected {
            message: "Codex 配置与登录缓存备份不属于同一次切换".to_string(),
            line: None,
        });
    }

    let mut auth_pre_restore = None;
    let mut state_commit = Some(commit);
    restore(io, backup, target, |outcome| {
        let auth_pre =
            restore_codex_auth(io, auth_target, auth_backup, &outcome.pre_restore_backup.id)?;
        auth_pre_restore = auth_pre;

        let state_commit = state_commit.take().expect("one state commit");
        if let Err(message) = state_commit(outcome) {
            if let Some(auth_pre) = auth_pre_restore.as_ref() {
                let recovery = restore_backup_content(io, auth_target, auth_pre);
                if !matches!(recovery, RecoveryOutcome::Restored { .. }) {
                    return Err(format!("{message}; Codex 登录缓存恢复失败"));
                }
            }
            return Err(message);
        }
        Ok(())
    })
}

fn verify_auth_snapshot<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    expected_content: &str,
    expected_existed: bool,
) -> Result<(), SwitchError> {
    let (found_content, found_existed) =
        read_auth_or_empty(io, target).map_err(|error| SwitchError::ReadCurrent {
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

fn back_up_codex_auth<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    backup_dir: &Path,
    current: &str,
    found_hash: &str,
    target_existed: bool,
    linked_backup_id: Option<&str>,
    reason: &str,
    require_valid_current: bool,
) -> Result<BackupRecord, SwitchError> {
    verify_auth_snapshot(io, target, current, target_existed)?;
    io.ensure_dir(backup_dir)
        .map_err(|error| SwitchError::CommitFailed {
            stage: "backup-dir",
            message: error.to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        })?;
    let timestamp = timestamp_name(io);
    let backup_path = backup_dir.join(format!("auth.json.{timestamp}.credential.bak"));
    let created_at = io.now_rfc3339();
    io.write_new_file(&backup_path, current)
        .map_err(|error| SwitchError::CommitFailed {
            stage: "backup",
            message: error.to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        })?;
    let backup_text = io
        .read_file(&backup_path)
        .map_err(|error| SwitchError::CommitFailed {
            stage: "backup-verify",
            message: error.to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        })?;
    if backup_text != current
        || (require_valid_current && adapter::validate_codex_auth(&backup_text).is_err())
    {
        return Err(SwitchError::CommitFailed {
            stage: "backup-verify",
            message: "Codex 登录缓存备份回读内容或 JSON 不匹配".to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        });
    }
    let backup = BackupRecord {
        id: format!(
            "auth-{}-{timestamp}",
            &found_hash[..12.min(found_hash.len())]
        ),
        app: AppKind::Codex,
        target_path: target.to_string_lossy().to_string(),
        backup_path: backup_path.to_string_lossy().to_string(),
        created_at,
        content_hash: found_hash.to_string(),
        target_existed,
        linked_backup_id: linked_backup_id.map(str::to_string),
        reason: reason.to_string(),
    };
    write_backup_metadata(io, &backup, "backup-meta")?;
    Ok(backup)
}

fn write_codex_auth<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    backup_dir: &Path,
    plan: &SwitchPlan,
    expected_hash: &str,
    expected_rendered_hash: &str,
    linked_backup_id: &str,
) -> Result<BackupRecord, String> {
    if let Some(parent) = target.parent() {
        io.ensure_dir(parent)
            .map_err(|error| format!("无法准备 Codex 登录缓存目录：{error}"))?;
    }
    let (current, existed) = read_auth_or_empty(io, target)
        .map_err(|error| format!("无法读取 Codex 登录缓存：{error}"))?;
    let found_hash = sha256_hex(&current);
    if found_hash != expected_hash {
        return Err("Codex 登录缓存已在预览后被外部修改".to_string());
    }
    let rendered = adapter::render_codex_auth(&current, plan)
        .map_err(|error| format!("Codex 登录缓存计划无效：{error}"))?;
    if sha256_hex(&rendered) != expected_rendered_hash {
        return Err("供应商或 Codex 登录缓存候选已变更，请重新查看差异".to_string());
    }
    let backup = back_up_codex_auth(
        io,
        target,
        backup_dir,
        &current,
        &found_hash,
        existed,
        Some(linked_backup_id),
        "provider-credential",
        true,
    )
    .map_err(|error| error.to_string())?;
    if rendered != current {
        replace_codex_auth(io, target, &current, existed, &rendered, &backup)?;
    }
    Ok(backup)
}

fn replace_codex_auth<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    current: &str,
    existed: bool,
    rendered: &str,
    backup: &BackupRecord,
) -> Result<(), String> {
    let file_name = target
        .file_name()
        .expect("auth target has a file name")
        .to_string_lossy()
        .to_string();
    let temporary =
        target.with_file_name(format!("{file_name}.{}.asb-auth-tmp", std::process::id()));
    io.write_new_file(&temporary, rendered)
        .map_err(|error| format!("无法写入 Codex 登录缓存临时文件：{error}"))?;
    let temporary_text = match io.read_file(&temporary) {
        Ok(text) if text == rendered && adapter::validate_codex_auth(&text).is_ok() => text,
        Ok(_) => {
            let _ = io.remove(&temporary);
            return Err("Codex 登录缓存临时文件回读或 JSON 校验失败".to_string());
        }
        Err(error) => {
            let _ = io.remove(&temporary);
            return Err(format!("无法回读 Codex 登录缓存临时文件：{error}"));
        }
    };
    if temporary_text != rendered {
        let _ = io.remove(&temporary);
        return Err("Codex 登录缓存临时文件回读不匹配".to_string());
    }
    if let Err(error) = verify_auth_snapshot(io, target, &current, existed) {
        let _ = io.remove(&temporary);
        return Err(error.to_string());
    }
    if let Err(error) = io.rename_replace(&temporary, target) {
        let _ = io.remove(&temporary);
        return Err(format!("无法原子替换 Codex 登录缓存：{error}"));
    }
    let verified = matches!(
        io.read_file(target),
        Ok(ref text) if text == rendered && adapter::validate_codex_auth(text).is_ok()
    );
    if !verified {
        let recovery = restore_backup_content(io, target, &backup);
        return Err(format!(
            "Codex 登录缓存写后校验失败，恢复结果：{}",
            recovery_label(&recovery)
        ));
    }
    Ok(())
}

fn restore_codex_auth<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    source_backup: &BackupRecord,
    linked_backup_id: &str,
) -> Result<Option<BackupRecord>, String> {
    let source = io
        .read_file(Path::new(&source_backup.backup_path))
        .map_err(|error| format!("Codex 登录缓存备份不可读：{error}"))?;
    if sha256_hex(&source) != source_backup.content_hash
        || (source_backup.target_existed && adapter::validate_codex_auth(&source).is_err())
    {
        return Err("Codex 登录缓存备份校验失败".to_string());
    }
    let (current, existed) = read_auth_or_empty(io, target)
        .map_err(|error| format!("无法读取 Codex 登录缓存：{error}"))?;
    if current == source && existed == source_backup.target_existed {
        return Ok(None);
    }
    if let Some(parent) = target.parent() {
        io.ensure_dir(parent)
            .map_err(|error| format!("无法准备 Codex 登录缓存目录：{error}"))?;
    }
    let current_hash = sha256_hex(&current);
    let backup_dir = Path::new(&source_backup.backup_path)
        .parent()
        .expect("credential backup has a parent");
    let pre_restore = back_up_codex_auth(
        io,
        target,
        backup_dir,
        &current,
        &current_hash,
        existed,
        Some(linked_backup_id),
        "credential-restore-precheck",
        false,
    )
    .map_err(|error| error.to_string())?;
    if !source_backup.target_existed {
        verify_auth_snapshot(io, target, &current, existed).map_err(|error| error.to_string())?;
        io.remove(target)
            .map_err(|error| format!("无法移除 Codex 登录缓存：{error}"))?;
        return match io.read_file(target) {
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Some(pre_restore)),
            Ok(_) => {
                let recovery = restore_backup_content(io, target, &pre_restore);
                Err(format!(
                    "移除 Codex 登录缓存后仍可读取，恢复结果：{}",
                    recovery_label(&recovery)
                ))
            }
            Err(error) => {
                let recovery = restore_backup_content(io, target, &pre_restore);
                Err(format!(
                    "无法确认 Codex 登录缓存删除结果：{error}；恢复结果：{}",
                    recovery_label(&recovery)
                ))
            }
        };
    }
    replace_codex_auth(io, target, &current, existed, &source, &pre_restore)?;
    Ok(Some(pre_restore))
}
