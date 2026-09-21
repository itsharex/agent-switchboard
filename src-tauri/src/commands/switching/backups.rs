use crate::commands::error::CommandError;
use asb_core::contracts::{AppKind, BackupRecord, ConfigWriteRecord, WriteOperation};
use asb_switch::io::{FsIo, SwitchIo};
use asb_switch::{list_backups as scan_backups, restore_projected, RestoreOutcome};
use std::path::PathBuf;

/// Newly created reset backups must satisfy the same route, catalog and auth
/// requirements as a restore, both before preview and under the commit gate.
pub(in crate::commands) fn validate_client_configuration_backup(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    app: AppKind,
    current: &str,
    existed: bool,
) -> Result<(), CommandError> {
    if !existed { return Ok(()); }
    let validate = || -> Result<(), String> {
        let candidate = gateway.prepare_restored(state, app, current)?;
        if app == AppKind::Codex {
            let projected = gateway.restored_codex_catalog(state, &candidate)?;
            let target = state.target(app)?;
            super::plan::catalog_artifact(&target, projected.as_ref()).map_err(|error| error.message)?;
            super::codex_restore_auth::validate_current_auth(&target, &candidate).map_err(|error| error.message)?;
        }
        Ok(())
    };
    validate().map_err(|message| CommandError::new(
        "client-configuration-not-restorable",
        format!("当前配置无法通过备份恢复校验，尚未重置：{message}"),
    ))
}

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
    let app = record.app;
    let candidate = validate_restore_source(state, gateway, record)?;
    let settings = super::client_settings_backup::load(state, record)
        .map_err(|error| CommandError::new("backup-settings-invalid", error))?;
    let before = state.configuration().get_client_settings(app).map_err(super::store_error)?;
    let catalog = if app == AppKind::Codex {
        let projected = gateway
            .restored_codex_catalog(state, &candidate)
            .map_err(|error| CommandError::new("backup-catalog-invalid", error))?;
        super::plan::catalog_artifact(&target, projected.as_ref())?
    } else {
        None
    };
    let auth = super::codex_restore_auth::intent(state, record, &target, &candidate)?;
    super::transaction::begin_restore(
        state,
        gateway,
        app,
        &asb_switch::sha256_hex(&candidate),
        record.target_existed,
        catalog.clone(),
        auth,
        &before,
        settings.as_ref().unwrap_or(&before.settings),
    )?;
    super::transaction::stage_catalog(state, gateway, catalog.as_ref())?;
    let execution = restore_projected(&FsIo, record, &target, Some(&candidate), |outcome| {
        gateway.reconcile_restored(state, app, || {
            super::transaction::commit_client_settings(state, &outcome.pre_restore_backup)?;
            state
                .configuration()
                .record_config_write(ConfigWriteRecord {
                    app,
                    profile_id: None,
                    profile_name: None,
                    content_hash: outcome.restored_hash.clone(),
                    backup_id: outcome.pre_restore_backup.id.clone(),
                    at: outcome.pre_restore_backup.created_at.clone(),
                    operation: WriteOperation::Restore,
                })
                .map_err(|error| error.to_string())
        })
    });
    super::transaction::finish(state, gateway, execution)
}

fn validate_restore_source(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    record: &BackupRecord,
) -> Result<String, CommandError> {
    let app = record.app;
    if app == AppKind::Codex
        && scan_backups(&FsIo, &state.backup_dir())
            .iter()
            .any(|candidate| {
                candidate.linked_backup_id.as_deref() == Some(record.id.as_str())
                    && candidate.reason != "codex-auth-projection"
            })
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
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_preflight_rejects_nonrestorable_sources_without_creating_backups() {
        let directory = tempfile::tempdir().unwrap();
        let state = crate::local_state::LocalState::from_root(directory.path().join("state"));
        let gateway_path = state.gateway_state_path();
        std::fs::create_dir_all(gateway_path.parent().unwrap()).unwrap();
        // Invalid isolated state keeps startup from binding or rehydrating client files.
        std::fs::write(&gateway_path, "invalid gateway state").unwrap();
        let gateway = crate::gateway::GatewayController::start(&state);
        for source in ["model_provider = 'personal'", "[model_providers.openai]\nbase_url = 'https://relay.example'"] {
            let error = validate_client_configuration_backup(&state, &gateway, AppKind::Codex, source, true).unwrap_err();
            assert_eq!(error.code, "client-configuration-not-restorable");
        }
        assert!(validate_client_configuration_backup(&state, &gateway, AppKind::Claude, "{}", true).is_ok());
        assert!(validate_client_configuration_backup(&state, &gateway, AppKind::Claude, "{", true).is_err());
        assert!(validate_client_configuration_backup(&state, &gateway, AppKind::Codex, "", false).is_ok());
        assert!(!state.backup_dir().exists());
        gateway.shutdown();
    }
}
