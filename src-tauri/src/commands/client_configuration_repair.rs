//! Safe, confirmed repair for malformed client configuration files.
//!
//! Repair candidates are deterministic normalizations only. The original file
//! is always hash-checked, backed up, and recoverable through the shared write
//! executor before the corrected candidate replaces it.

use super::{
    error::{blocking, require_write_confirmation, state, CommandError},
    ConfigWriteGate,
};
use asb_core::contracts::{AppKind, ConfigWriteRecord, WriteOperation};
use asb_switch::{
    execute_rendered, preview_repair_rendered, repair_invalid_configuration, sha256_hex, FsIo,
    RenderedWriteRequest,
};
use serde::Serialize;
use std::io::ErrorKind;
use tauri::{AppHandle, Manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfigurationRepairPreview {
    pub file: asb_switch::FilePreview,
    pub target_existed: bool,
}

fn candidate(
    state: &crate::local_state::LocalState,
    target: AppKind,
    expected_source_hash: &str,
) -> Result<(String, bool, String), CommandError> {
    let path = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let current = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(_) => return Err(CommandError::keyed("client-configuration-unreadable", "errors.cfg.clientConfigUnreadable", "无法读取真实客户端配置文件")),
    };
    if sha256_hex(&current) != expected_source_hash {
        return Err(CommandError::keyed("client-configuration-preview-stale", "errors.cfg.repairPreviewStale", "真实配置已变化，请重新读取后再修复"));
    }
    let rendered = repair_invalid_configuration(target, &current).map_err(CommandError::from)?;
    Ok((current, path.exists(), rendered))
}

#[tauri::command]
pub async fn preview_client_configuration_repair(
    app: AppHandle,
    target: AppKind,
    expected_source_hash: String,
) -> Result<ClientConfigurationRepairPreview, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let (current, existed, rendered) = candidate(&state, target, &expected_source_hash)?;
        let path = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
        let file = preview_repair_rendered(target, &path, &state.backup_dir(), &current, &rendered)
            .map_err(CommandError::from)?;
        Ok(ClientConfigurationRepairPreview { file, target_existed: existed })
    }).await
}

#[tauri::command]
pub async fn commit_client_configuration_repair(
    app: AppHandle,
    target: AppKind,
    expected_source_hash: String,
    expected_rendered_hash: String,
    expected_target_existed: bool,
    confirm_write: bool,
) -> Result<(), CommandError> {
    require_write_confirmation(confirm_write, "修复客户端配置")?;
    let state = state(&app)?;
    let gate = app.try_state::<ConfigWriteGate>()
        .ok_or_else(|| CommandError::keyed("app-state-unavailable", "errors.cfg.writeGateNotInitialized", "写入闸门尚未初始化"))?
        .inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(|error| CommandError::new("config-write-gate-unavailable", error))?;
        crate::commands::switching::ensure_profile_save_recovered(&app)?;
        let (current, existed, rendered) = candidate(&state, target, &expected_source_hash)?;
        if sha256_hex(&rendered) != expected_rendered_hash || existed != expected_target_existed {
            return Err(CommandError::keyed("client-configuration-preview-stale", "errors.cfg.repairCandidateStale", "真实配置或修复候选已变化，请重新预览"));
        }
        let file_target = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
        let backup_dir = state.backup_dir();
        let config = state.configuration();
        let gateway = app.state::<crate::gateway::GatewayController>();
        super::switching::transaction::begin_client_configuration(
            &state, gateway.inner(), target, &expected_rendered_hash, existed || current != rendered, None,
        )?;
        let execution = execute_rendered(&FsIo, &RenderedWriteRequest {
            target: &file_target,
            app: target,
            backup_dir: &backup_dir,
            expected_hash: &expected_source_hash,
            expected_target_existed,
            rendered: &rendered,
            reason: "client-configuration-repair",
        }, |outcome| {
            config.record_config_write(ConfigWriteRecord {
                app: target,
                profile_id: None,
                profile_name: None,
                content_hash: outcome.final_hash.clone(),
                backup_id: outcome.backup.id.clone(),
                at: outcome.backup.created_at.clone(),
                operation: WriteOperation::Projection,
            }).map_err(|error| error.to_string())
        });
        super::switching::transaction::finish(&state, gateway.inner(), execution)?;
        Ok(())
    }).await
}
