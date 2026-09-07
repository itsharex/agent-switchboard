use crate::commands::error::CommandError;
use asb_core::contracts::{AppKind, BackupRecord, ConfigWriteRecord, WriteOperation};
use asb_switch::io::FsIo;
use asb_switch::{list_backups as scan_backups, restore, restore_codex, RestoreOutcome};
use std::path::PathBuf;

pub(super) fn local_backups(
    state: &crate::local_state::LocalState,
) -> Result<Vec<BackupRecord>, CommandError> {
    let targets = [AppKind::Codex, AppKind::Claude]
        .into_iter()
        .map(|app| {
            state
                .target(app)
                .map(|target| (app, target))
                .map_err(|error| CommandError::new("config-path-unavailable", error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(scan_backups(&FsIo, &state.backup_dir())
        .into_iter()
        .filter(|record| {
            targets.iter().any(|(app, target)| {
                record.app == *app
                    && PathBuf::from(&record.target_path).as_path() == target.as_path()
            }) && record.linked_backup_id.is_none()
        })
        .collect())
}

pub(super) fn find_backup(
    state: &crate::local_state::LocalState,
    backup_id: &str,
) -> Result<BackupRecord, CommandError> {
    local_backups(state)?
        .into_iter()
        .find(|record| record.id == backup_id)
        .ok_or_else(|| CommandError::new("backup-not-found", "找不到指定备份"))
}

/// Credential backups are hidden from the standalone backup list because they
/// are meaningful only with their linked Codex configuration snapshot.
fn linked_codex_auth_backup(
    state: &crate::local_state::LocalState,
    record: &BackupRecord,
) -> Result<BackupRecord, CommandError> {
    if record.app != AppKind::Codex {
        return Err(CommandError::new(
            "codex-auth-backup-invalid",
            "只有 Codex 配置备份可以关联 auth.json 快照",
        ));
    }
    let auth_target = crate::local_state::LocalState::codex_auth_path()
        .map_err(|error| CommandError::new("codex-auth-path-unavailable", error))?;
    scan_backups(&FsIo, &state.backup_dir())
        .into_iter()
        .find(|candidate| {
            candidate.app == AppKind::Codex
                && candidate.linked_backup_id.as_deref() == Some(record.id.as_str())
                && PathBuf::from(&candidate.target_path) == auth_target
        })
        .ok_or_else(|| {
            CommandError::new(
                "codex-auth-backup-missing",
                "Codex 配置备份缺少同次切换的 auth.json 快照，已拒绝只恢复配置以避免端点与密钥错配",
            )
        })
}

/// Shared restore path: validates the target, restores, and records the
/// operation in the switch log so it can be undone in turn.
pub(super) fn run_restore(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    record: &BackupRecord,
) -> Result<RestoreOutcome, CommandError> {
    if matches!(
        record.reason.as_str(),
        "gateway-port-change" | "gateway-port-rollback"
    ) {
        return Err(CommandError::new(
            "gateway-port-restore-unavailable",
            "网关端口修改涉及全部客户端与监听器，请在网关页修改端口，不能恢复单个客户端配置",
        ));
    }
    let target = state
        .target(record.app)
        .map_err(|error| CommandError::new("config-path-unavailable", error))?;
    if PathBuf::from(&record.target_path) != target {
        return Err(CommandError::new(
            "backup-target-invalid",
            "备份不属于当前本机配置路径，已拒绝恢复",
        ));
    }
    let write_record = ConfigWriteRecord {
        app: record.app,
        profile_id: None,
        profile_name: None,
        content_hash: String::new(),
        backup_id: String::new(),
        at: String::new(),
        operation: WriteOperation::Restore,
    };
    let outcome = match record.app {
        AppKind::Codex => {
            let auth_backup = linked_codex_auth_backup(state, record)?;
            let auth_target = crate::local_state::LocalState::codex_auth_path()
                .map_err(|error| CommandError::new("codex-auth-path-unavailable", error))?;
            restore_codex(
                &FsIo,
                record,
                &auth_backup,
                &target,
                &auth_target,
                move |outcome| {
                    gateway.reconcile_restored(state, AppKind::Codex, || {
                        state
                            .configuration()
                            .record_config_write(ConfigWriteRecord {
                                content_hash: outcome.restored_hash.clone(),
                                backup_id: outcome.pre_restore_backup.id.clone(),
                                at: chrono::Utc::now()
                                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                                ..write_record
                            })
                    })
                },
            )
        }
        AppKind::Claude => restore(&FsIo, record, &target, move |outcome| {
            gateway.reconcile_restored(state, AppKind::Claude, || {
                state
                    .configuration()
                    .record_config_write(ConfigWriteRecord {
                        content_hash: outcome.restored_hash.clone(),
                        backup_id: outcome.pre_restore_backup.id.clone(),
                        at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                        ..write_record
                    })
            })
        }),
    };
    outcome.map_err(CommandError::from)
}
