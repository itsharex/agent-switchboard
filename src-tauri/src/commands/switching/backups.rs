use crate::commands::error::CommandError;
use asb_core::contracts::{AppKind, BackupRecord, ConfigWriteRecord, WriteOperation};
use asb_switch::io::{FsIo, SwitchIo};
use asb_switch::{list_backups as scan_backups, restore_projected, RestoreOutcome};
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
    let app = record.app;
    let candidate = validate_restore_source(state, gateway, record, &target)?;
    super::transaction::begin(
        state,
        gateway,
        app,
        None,
        &asb_switch::sha256_hex(&candidate),
        record.target_existed,
    )?;
    let execution = restore_projected(&FsIo, record, &target, Some(&candidate), |outcome| {
        gateway.reconcile_restored(state, app, || {
            state
                .configuration()
                .record_config_write(ConfigWriteRecord {
                    content_hash: outcome.restored_hash.clone(),
                    backup_id: outcome.pre_restore_backup.id.clone(),
                    at: outcome.pre_restore_backup.created_at.clone(),
                    ..write_record
                })
        })
    });
    super::transaction::finish(state, gateway, execution)
}

fn validate_restore_source(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    record: &BackupRecord,
    target: &std::path::Path,
) -> Result<String, CommandError> {
    let app = record.app;
    if app == AppKind::Codex
        && scan_backups(&FsIo, &state.backup_dir())
            .iter()
            .any(|candidate| candidate.linked_backup_id.as_deref() == Some(record.id.as_str()))
    {
        return Err(CommandError::new(
            "backup-contract-retired",
            "旧版认证联动备份只能查看或导出，当前切换器不会恢复登录缓存",
        ));
    }
    let candidate = FsIo
        .read_file(PathBuf::from(&record.backup_path).as_path())
        .map_err(|_| CommandError::new("backup-unreadable", "无法读取备份"))?;
    if asb_switch::sha256_hex(&candidate) != record.content_hash {
        return Err(CommandError::new(
            "backup-hash-mismatch",
            "备份内容与记录不一致",
        ));
    }
    let candidate = if record.target_existed {
        gateway
            .prepare_restored(state, app, &candidate)
            .map_err(|error| CommandError::new("backup-route-invalid", error))?
    } else {
        candidate
    };
    if app == AppKind::Codex
        && candidate
            .parse::<toml_edit::DocumentMut>()
            .ok()
            .and_then(|doc| {
                doc.get("openai_base_url")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
            })
            .is_some()
    {
        crate::official_login::observation::observe_codex_login_for_config(target, &candidate)
            .require()
            .map_err(|error| CommandError::new("codex-official-login-required", error))?;
    }
    Ok(candidate)
}
