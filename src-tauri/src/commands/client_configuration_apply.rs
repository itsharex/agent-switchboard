//! One confirmed transaction for applying all client-configuration drafts.
use super::{
    error::{blocking, require_write_confirmation, state, store_error, CommandError},
    ConfigWriteGate,
};
use asb_core::{contracts::{AppKind, ConfigWriteRecord, SettingsValues, WriteOperation}};
use asb_switch::{execute_rendered, preview_rendered, sha256_hex, FsIo, RenderedWriteRequest};
use serde::Serialize;
use std::io::ErrorKind;
use tauri::{AppHandle, Manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfigurationApplyPreview {
    pub file: asb_switch::FilePreview,
    pub settings_hash: String,
    pub target_existed: bool,
}

fn candidate(
    state: &crate::local_state::LocalState,
    target: AppKind,
    settings: SettingsValues,
    subagent_settings: Option<asb_core::CodexSubagentSettings>,
) -> Result<(String, bool, SettingsValues, String), CommandError> {
    let path = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let current = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(_) => return Err(CommandError::new("client-configuration-unreadable", "无法读取真实客户端配置文件")),
    };
    settings.validate_client_settings(target)
        .map_err(|error| CommandError::new("client-configuration-rejected", error.to_string()))?;
    let document = if current.is_empty() && target == AppKind::Claude { "{}" } else { &current };
    let rendered = asb_core::adapter::render_client_settings_into_file(target, document, &settings)
        .map_err(|error| CommandError::new("client-configuration-preview-failed", error.to_string()))?;
    let rendered = if target == AppKind::Codex {
        let subagent_settings = subagent_settings.ok_or_else(|| CommandError::new(
            "client-configuration-rejected",
            "Codex 客户端通用配置缺少子 agent 运行设置",
        ))?;
        asb_core::adapter::codex::render_subagent_settings(&rendered, &subagent_settings)
            .map_err(|error| CommandError::new("client-configuration-preview-failed", error.to_string()))?
    } else if subagent_settings.is_some() {
        return Err(CommandError::new("client-configuration-rejected", "Claude 不接受子 agent 运行设置"));
    } else { rendered };
    Ok((current, path.exists(), settings, rendered))
}

#[tauri::command]
pub async fn preview_client_configuration_apply(
    app: AppHandle,
    target: AppKind,
    settings: SettingsValues,
    subagent_settings: Option<asb_core::CodexSubagentSettings>,
) -> Result<ClientConfigurationApplyPreview, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let stored = state.configuration().get_client_settings(target).map_err(store_error)?;
        let (current, existed, _, rendered) = candidate(&state, target, settings, subagent_settings)?;
        let path = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
        let file = preview_rendered(target, &path, &state.backup_dir(), &current, &rendered)
            .map_err(CommandError::from)?;
        Ok(ClientConfigurationApplyPreview { file, settings_hash: stored.settings_hash, target_existed: existed })
    }).await
}

#[tauri::command]
pub async fn commit_client_configuration_apply(
    app: AppHandle,
    target: AppKind,
    expected_hash: String,
    expected_rendered_hash: String,
    expected_settings_hash: String,
    expected_target_existed: bool,
    settings: SettingsValues,
    subagent_settings: Option<asb_core::CodexSubagentSettings>,
    confirm_write: bool,
) -> Result<(), CommandError> {
    require_write_confirmation(confirm_write, "应用客户端通用配置")?;
    let state = state(&app)?;
    let gate = app.try_state::<ConfigWriteGate>()
        .ok_or_else(|| CommandError::new("app-state-unavailable", "写入闸门尚未初始化"))?
        .inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(|error| CommandError::new("config-write-gate-unavailable", error))?;
        crate::commands::switching::ensure_profile_save_recovered(&app)?;
        let (current, existed, settings, rendered) = candidate(&state, target, settings, subagent_settings)?;
        if sha256_hex(&current) != expected_hash || sha256_hex(&rendered) != expected_rendered_hash || existed != expected_target_existed {
            return Err(CommandError::new("client-configuration-preview-stale", "真实配置或候选已变化，请重新预览"));
        }
        let config = state.configuration();
        let before = config.get_client_settings(target).map_err(store_error)?;
        if before.settings_hash != expected_settings_hash {
            return Err(CommandError::new("client-configuration-preview-stale", "通用配置意图已变化，请重新预览"));
        }
        let file_target = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
        let backup_dir = state.backup_dir();
        execute_rendered(&FsIo, &RenderedWriteRequest {
            target: &file_target,
            app: target,
            backup_dir: &backup_dir,
            expected_hash: &expected_hash,
            expected_target_existed,
            rendered: &rendered,
            reason: "client-configuration-apply",
        }, |outcome| {
            let saved = config.save_client_settings(target, settings.clone(), &before.settings_hash)
                .map_err(|error| error.to_string())?;
            if let Err(error) = config.record_config_write(ConfigWriteRecord {
                app: target,
                profile_id: None,
                profile_name: Some("客户端通用配置".into()),
                content_hash: outcome.final_hash.clone(),
                backup_id: outcome.backup.id.clone(),
                at: outcome.backup.created_at.clone(),
                operation: WriteOperation::Projection,
            }) {
                config.save_client_settings(target, before.settings.clone(), &saved.settings_hash)
                    .map_err(|rollback| format!("{error}；恢复客户端通用配置意图失败：{rollback}"))?;
                return Err(error.to_string());
            }
            Ok(())
        }).map_err(CommandError::from)?;
        Ok(())
    }).await
}
