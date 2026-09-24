//! Hash-bound client configuration application and scoped reset transactions.
use super::{
    error::{blocking, require_write_confirmation, state, store_error, CommandError},
    ConfigWriteGate,
};
use asb_core::{
    adapter::PreviewDiff,
    contracts::{AppKind, CodexSubagentSettings, ConfigWriteRecord, SettingsValues, WriteOperation},
    ownership,
};
use asb_switch::{execute_rendered, preview_rendered, sha256_hex, FsIo, RenderedWriteRequest};
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use tauri::{AppHandle, Manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfigurationApplyPreview {
    pub file: asb_switch::FilePreview,
    pub settings_hash: String,
    pub target_existed: bool,
}

/// The non-draft client-configuration mutations. The backend owns their
/// exact scopes so a UI action cannot accidentally persist unrelated edits.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientConfigurationResetKind {
    NativeDefaults,
    NativeDefaultsWithUnmanaged,
    ClearExtraConfiguration,
}

impl ClientConfigurationResetKind {
    fn settings(
        self,
        target: AppKind,
        saved: SettingsValues,
    ) -> Result<SettingsValues, CommandError> {
        match self {
            Self::NativeDefaults | Self::NativeDefaultsWithUnmanaged => {
                let mut settings = ownership::default_client_settings(target);
                if target == AppKind::Claude {
                    settings.claude_extra = saved.claude_extra;
                }
                Ok(settings)
            }
            Self::ClearExtraConfiguration => {
                if target != AppKind::Claude {
                    return Err(CommandError::keyed(
                        "client-configuration-rejected",
                        "errors.cfg.clearExtraClaudeOnly",
                        "仅 Claude 支持清空 ASB 管理的额外通用配置",
                    ));
                }
                let mut settings = saved;
                settings.claude_extra.clear();
                Ok(settings)
            }
        }
    }

    fn write_label(self) -> &'static str {
        match self {
            Self::NativeDefaults => "恢复客户端原生默认值",
            Self::NativeDefaultsWithUnmanaged => "恢复客户端原生默认值并移除界面外字段",
            Self::ClearExtraConfiguration => "清空 ASB 管理的额外通用配置",
        }
    }

    fn backup_reason(self) -> &'static str {
        match self {
            Self::NativeDefaults => "client-configuration-native-defaults",
            Self::NativeDefaultsWithUnmanaged => "client-configuration-native-defaults-unmanaged",
            Self::ClearExtraConfiguration => "client-configuration-clear-extra-configuration",
        }
    }

    /// The deep reset deletes host-owned leaves, so its preview must list
    /// them; the other scopes only ever remove owned keys.
    fn diff_scope(self) -> PreviewDiff {
        match self {
            Self::NativeDefaults | Self::ClearExtraConfiguration => PreviewDiff::Owned,
            Self::NativeDefaultsWithUnmanaged => PreviewDiff::Full,
        }
    }
}

pub(super) fn render_client_configuration(
    target: AppKind,
    current: &str,
    settings: &SettingsValues,
    subagent_settings: Option<&CodexSubagentSettings>,
) -> Result<String, CommandError> {
    settings.validate_client_settings(target)
        .map_err(|error| CommandError::new("client-configuration-rejected", error.to_string()))?;
    let document = if current.is_empty() && target == AppKind::Claude { "{}" } else { current };
    let rendered = asb_core::adapter::render_client_settings_into_file(target, document, settings)
        .map_err(|error| CommandError::new("client-configuration-preview-failed", error.to_string()))?;
    if target == AppKind::Codex {
        let subagent_settings = subagent_settings.ok_or_else(|| CommandError::keyed(
            "client-configuration-rejected",
            "errors.cfg.codexSubagentSettingsMissing",
            "Codex 客户端配置缺少子 agent 运行设置",
        ))?;
        asb_core::adapter::codex::render_subagent_settings(&rendered, subagent_settings)
            .map_err(|error| CommandError::new("client-configuration-preview-failed", error.to_string()))
    } else if subagent_settings.is_some() {
        Err(CommandError::keyed("client-configuration-rejected", "errors.cfg.claudeRejectsSubagentSettings", "Claude 不接受子 agent 运行设置"))
    } else {
        Ok(rendered)
    }
}

fn candidate(
    state: &crate::local_state::LocalState,
    target: AppKind,
    settings: SettingsValues,
    subagent_settings: Option<CodexSubagentSettings>,
) -> Result<(String, bool, SettingsValues, String), CommandError> {
    let path = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let current = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => match target {
            AppKind::Codex => String::new(),
            AppKind::Claude => "{}".to_string(),
        },
        Err(_) => return Err(CommandError::keyed("client-configuration-unreadable", "errors.cfg.clientConfigUnreadable", "无法读取真实客户端配置文件")),
    };
    let rendered = render_client_configuration(target, &current, &settings, subagent_settings.as_ref())?;
    Ok((current, path.exists(), settings, rendered))
}

pub(super) fn commit_rendered_client_configuration(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    target: AppKind,
    current: &str,
    existed: bool,
    settings: SettingsValues,
    rendered: &str,
    expected_hash: &str,
    expected_rendered_hash: &str,
    expected_settings_hash: &str,
    expected_target_existed: bool,
    reason: &'static str,
) -> Result<(), CommandError> {
    if sha256_hex(current) != expected_hash ||
        sha256_hex(rendered) != expected_rendered_hash ||
        existed != expected_target_existed {
        return Err(CommandError::keyed("client-configuration-preview-stale", "errors.cfg.previewStaleTargetChanged", "真实配置或候选已变化，请重新预览"));
    }
    let config = state.configuration();
    let before = config.get_client_settings(target).map_err(store_error)?;
    if before.settings_hash != expected_settings_hash {
        return Err(CommandError::keyed("client-configuration-preview-stale", "errors.cfg.previewStaleSettingsChanged", "通用配置意图已变化，请重新预览"));
    }
    if current == rendered && before.settings == settings {
        return Ok(());
    }
    let file_target = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let backup_dir = state.backup_dir();
    super::switching::transaction::begin_client_configuration(
        state, gateway, target, expected_rendered_hash, existed || current != rendered, Some((&before, &settings)),
    )?;
    let execution = execute_rendered(&FsIo, &RenderedWriteRequest {
        target: &file_target,
        app: target,
        backup_dir: &backup_dir,
        expected_hash,
        expected_target_existed,
        rendered,
        reason,
    }, |outcome| {
        super::switching::transaction::commit_client_settings(state, &outcome.backup)?;
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
    super::switching::transaction::finish(state, gateway, execution)?;
    Ok(())
}

fn render_reset_configuration(
    target: AppKind,
    current: &str,
    kind: ClientConfigurationResetKind,
    settings: &SettingsValues,
    codex_fragment: &str,
) -> Result<String, CommandError> {
    match kind {
        ClientConfigurationResetKind::NativeDefaults => {
            asb_core::adapter::reset_client_settings(target, current)
                .map_err(|error| CommandError::new("client-configuration-preview-failed", error.to_string()))
        }
        ClientConfigurationResetKind::NativeDefaultsWithUnmanaged => {
            let pruned = asb_core::adapter::remove_unmanaged_entries(target, current, codex_fragment)
                .map_err(|error| {
                    CommandError::new("client-configuration-preview-failed", error.to_string())
                })?;
            asb_core::adapter::reset_client_settings(target, &pruned)
                .map_err(|error| CommandError::new("client-configuration-preview-failed", error.to_string()))
        }
        ClientConfigurationResetKind::ClearExtraConfiguration => {
            settings.validate_client_settings(target)
                .map_err(|error| CommandError::new("client-configuration-rejected", error.to_string()))?;
            let document = if current.is_empty() { "{}" } else { current };
            asb_core::claude_common::apply(document, &settings.claude_extra).map_err(|error| {
                CommandError::new("client-configuration-preview-failed", error)
            })
        }
    }
}

fn reset_candidate(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    target: AppKind,
    kind: ClientConfigurationResetKind,
    saved: SettingsValues,
) -> Result<(String, bool, SettingsValues, String), CommandError> {
    let path = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let current = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => match target {
            AppKind::Codex => String::new(),
            AppKind::Claude => "{}".to_string(),
        },
        Err(_) => return Err(CommandError::keyed("client-configuration-unreadable", "errors.cfg.clientConfigUnreadable", "无法读取真实客户端配置文件")),
    };
    let existed = path.exists();
    super::switching::validate_client_configuration_backup(state, gateway, target, &current, existed)?;
    let settings = kind.settings(target, saved)?;
    let fragment = if target == AppKind::Codex && matches!(kind, ClientConfigurationResetKind::NativeDefaultsWithUnmanaged) {
        crate::codex_common::view_fragment(state.root())
            .map_err(|error| CommandError::new("client-configuration-preview-failed", error))?.text
    } else { String::new() };
    let rendered = render_reset_configuration(target, &current, kind, &settings, &fragment)?;
    Ok((current, existed, settings, rendered))
}

fn preview_reset(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    target: AppKind,
    kind: ClientConfigurationResetKind,
) -> Result<ClientConfigurationApplyPreview, CommandError> {
    let stored = state.configuration().get_client_settings(target).map_err(store_error)?;
    let (current, existed, _, rendered) = reset_candidate(state, gateway, target, kind, stored.settings.clone())?;
    let path = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let file = preview_rendered(
        target,
        &path,
        &state.backup_dir(),
        &current,
        &rendered,
        kind.diff_scope(),
    )
    .map_err(CommandError::from)?;
    Ok(ClientConfigurationApplyPreview { file, settings_hash: stored.settings_hash, target_existed: existed })
}

#[tauri::command]
pub async fn preview_client_configuration_apply(
    app: AppHandle,
    target: AppKind,
    settings: SettingsValues,
    subagent_settings: Option<CodexSubagentSettings>,
) -> Result<ClientConfigurationApplyPreview, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let stored = state.configuration().get_client_settings(target).map_err(store_error)?;
        let (current, existed, _, rendered) = candidate(&state, target, settings, subagent_settings)?;
        let path = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
        let file = preview_rendered(
            target,
            &path,
            &state.backup_dir(),
            &current,
            &rendered,
            PreviewDiff::Owned,
        )
        .map_err(CommandError::from)?;
        Ok(ClientConfigurationApplyPreview { file, settings_hash: stored.settings_hash, target_existed: existed })
    }).await
}

#[tauri::command]
pub async fn preview_client_configuration_reset(
    app: AppHandle,
    target: AppKind,
    reset_kind: ClientConfigurationResetKind,
) -> Result<ClientConfigurationApplyPreview, CommandError> {
    let state = state(&app)?;
    let gateway = app.state::<crate::gateway::GatewayController>().inner().clone();
    blocking(move || preview_reset(&state, &gateway, target, reset_kind)).await
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
    subagent_settings: Option<CodexSubagentSettings>,
    confirm_write: bool,
) -> Result<(), CommandError> {
    require_write_confirmation(confirm_write, "应用客户端配置")?;
    let state = state(&app)?;
    let gate = app.try_state::<ConfigWriteGate>()
        .ok_or_else(|| CommandError::keyed("app-state-unavailable", "errors.cfg.writeGateNotInitialized", "写入闸门尚未初始化"))?
        .inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(|error| CommandError::new("config-write-gate-unavailable", error))?;
        crate::commands::switching::ensure_profile_save_recovered(&app)?;
        let (current, existed, settings, rendered) = candidate(&state, target, settings, subagent_settings)?;
        commit_rendered_client_configuration(
            &state,
            app.state::<crate::gateway::GatewayController>().inner(),
            target,
            &current,
            existed,
            settings,
            &rendered,
            &expected_hash,
            &expected_rendered_hash,
            &expected_settings_hash,
            expected_target_existed,
            "client-configuration-apply",
        )
    }).await
}

#[tauri::command]
pub async fn commit_client_configuration_reset(
    app: AppHandle,
    target: AppKind,
    reset_kind: ClientConfigurationResetKind,
    expected_hash: String,
    expected_rendered_hash: String,
    expected_settings_hash: String,
    expected_target_existed: bool,
    confirm_write: bool,
) -> Result<(), CommandError> {
    require_write_confirmation(confirm_write, reset_kind.write_label())?;
    let state = state(&app)?;
    let gate = app.try_state::<ConfigWriteGate>()
        .ok_or_else(|| CommandError::keyed("app-state-unavailable", "errors.cfg.writeGateNotInitialized", "写入闸门尚未初始化"))?
        .inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(|error| CommandError::new("config-write-gate-unavailable", error))?;
        crate::commands::switching::ensure_profile_save_recovered(&app)?;
        let stored = state.configuration().get_client_settings(target).map_err(store_error)?;
        let (current, existed, settings, rendered) = reset_candidate(
            &state,
            app.state::<crate::gateway::GatewayController>().inner(),
            target,
            reset_kind,
            stored.settings,
        )?;
        commit_rendered_client_configuration(
            &state,
            app.state::<crate::gateway::GatewayController>().inner(),
            target,
            &current,
            existed,
            settings,
            &rendered,
            &expected_hash,
            &expected_rendered_hash,
            &expected_settings_hash,
            expected_target_existed,
            reset_kind.backup_reason(),
        )
    }).await
}

#[cfg(test)]
#[path = "client_configuration_apply_tests.rs"]
mod tests;
