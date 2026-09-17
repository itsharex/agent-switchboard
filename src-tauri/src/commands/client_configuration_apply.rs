//! Hash-bound client configuration application and scoped reset transactions.
use super::{
    error::{blocking, operation_error, require_write_confirmation, state, store_error, CommandError},
    ConfigWriteGate,
};
use asb_core::{
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

/// The two non-draft client-configuration mutations. The backend owns their
/// exact scopes so a UI action cannot accidentally persist unrelated edits.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientConfigurationResetKind {
    NativeDefaults,
    ClearExtraConfiguration,
}

impl ClientConfigurationResetKind {
    fn settings(
        self,
        target: AppKind,
        saved: SettingsValues,
    ) -> Result<(SettingsValues, Option<CodexSubagentSettings>), CommandError> {
        match self {
            Self::NativeDefaults => {
                let mut settings = ownership::default_client_settings(target);
                if target == AppKind::Claude {
                    settings.claude_extra = saved.claude_extra;
                }
                Ok((
                    settings,
                    (target == AppKind::Codex).then(CodexSubagentSettings::automatic),
                ))
            }
            Self::ClearExtraConfiguration => {
                if target != AppKind::Claude {
                    return Err(CommandError::new(
                        "client-configuration-rejected",
                        "仅 Claude 支持清空 ASB 管理的额外通用配置",
                    ));
                }
                let mut settings = saved;
                settings.claude_extra.clear();
                Ok((settings, None))
            }
        }
    }

    fn write_label(self) -> &'static str {
        match self {
            Self::NativeDefaults => "恢复客户端原生默认值",
            Self::ClearExtraConfiguration => "清空 ASB 管理的额外通用配置",
        }
    }

    fn backup_reason(self) -> &'static str {
        match self {
            Self::NativeDefaults => "client-configuration-native-defaults",
            Self::ClearExtraConfiguration => "client-configuration-clear-extra-configuration",
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
        let subagent_settings = subagent_settings.ok_or_else(|| CommandError::new(
            "client-configuration-rejected",
            "Codex 客户端通用配置缺少子 agent 运行设置",
        ))?;
        asb_core::adapter::codex::render_subagent_settings(&rendered, subagent_settings)
            .map_err(|error| CommandError::new("client-configuration-preview-failed", error.to_string()))
    } else if subagent_settings.is_some() {
        Err(CommandError::new("client-configuration-rejected", "Claude 不接受子 agent 运行设置"))
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
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(_) => return Err(CommandError::new("client-configuration-unreadable", "无法读取真实客户端配置文件")),
    };
    let rendered = render_client_configuration(target, &current, &settings, subagent_settings.as_ref())?;
    Ok((current, path.exists(), settings, rendered))
}

fn rendered_matches_current(target: AppKind, current: &str, rendered: &str) -> bool {
    current == rendered || (target == AppKind::Claude && current.is_empty() && rendered == "{}")
}

pub(super) fn commit_rendered_client_configuration(
    state: &crate::local_state::LocalState,
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
        return Err(CommandError::new("client-configuration-preview-stale", "真实配置或候选已变化，请重新预览"));
    }
    let config = state.configuration();
    let before = config.get_client_settings(target).map_err(store_error)?;
    if before.settings_hash != expected_settings_hash {
        return Err(CommandError::new("client-configuration-preview-stale", "通用配置意图已变化，请重新预览"));
    }
    if rendered_matches_current(target, current, rendered) {
        config
            .save_client_settings(target, settings, &before.settings_hash)
            .map_err(|error| operation_error("client-settings-save-failed", error))?;
        return Ok(());
    }
    let file_target = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let backup_dir = state.backup_dir();
    execute_rendered(&FsIo, &RenderedWriteRequest {
        target: &file_target,
        app: target,
        backup_dir: &backup_dir,
        expected_hash,
        expected_target_existed,
        rendered,
        reason,
    }, |outcome| {
        let saved = config.save_client_settings(target, settings.clone(), &before.settings_hash)
            .map_err(|error| error.to_string())?;
        if let Err(error) = config.record_config_write(ConfigWriteRecord {
            app: target,
            profile_id: None,
            profile_name: None,
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
}

fn render_reset_configuration(
    target: AppKind,
    current: &str,
    kind: ClientConfigurationResetKind,
    settings: &SettingsValues,
    subagent_settings: Option<&CodexSubagentSettings>,
) -> Result<String, CommandError> {
    match kind {
        ClientConfigurationResetKind::NativeDefaults => {
            render_client_configuration(target, current, settings, subagent_settings)
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
    target: AppKind,
    kind: ClientConfigurationResetKind,
    saved: SettingsValues,
) -> Result<(String, bool, SettingsValues, String), CommandError> {
    let path = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let current = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(_) => return Err(CommandError::new("client-configuration-unreadable", "无法读取真实客户端配置文件")),
    };
    let (settings, subagent_settings) = kind.settings(target, saved)?;
    let rendered = render_reset_configuration(
        target,
        &current,
        kind,
        &settings,
        subagent_settings.as_ref(),
    )?;
    Ok((current, path.exists(), settings, rendered))
}

fn preview_reset(
    state: &crate::local_state::LocalState,
    target: AppKind,
    kind: ClientConfigurationResetKind,
) -> Result<ClientConfigurationApplyPreview, CommandError> {
    let stored = state.configuration().get_client_settings(target).map_err(store_error)?;
    let (current, existed, _, rendered) = reset_candidate(state, target, kind, stored.settings.clone())?;
    let path = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let file = preview_rendered(target, &path, &state.backup_dir(), &current, &rendered)
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
        let file = preview_rendered(target, &path, &state.backup_dir(), &current, &rendered)
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
    blocking(move || preview_reset(&state, target, reset_kind)).await
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
    require_write_confirmation(confirm_write, "应用客户端通用配置")?;
    let state = state(&app)?;
    let gate = app.try_state::<ConfigWriteGate>()
        .ok_or_else(|| CommandError::new("app-state-unavailable", "写入闸门尚未初始化"))?
        .inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(|error| CommandError::new("config-write-gate-unavailable", error))?;
        crate::commands::switching::ensure_profile_save_recovered(&app)?;
        let (current, existed, settings, rendered) = candidate(&state, target, settings, subagent_settings)?;
        commit_rendered_client_configuration(
            &state,
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
        .ok_or_else(|| CommandError::new("app-state-unavailable", "写入闸门尚未初始化"))?
        .inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(|error| CommandError::new("config-write-gate-unavailable", error))?;
        crate::commands::switching::ensure_profile_save_recovered(&app)?;
        let stored = state.configuration().get_client_settings(target).map_err(store_error)?;
        let (current, existed, settings, rendered) = reset_candidate(
            &state,
            target,
            reset_kind,
            stored.settings,
        )?;
        commit_rendered_client_configuration(
            &state,
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
mod tests {
    use super::*;
    use asb_core::{contracts::{ConfigValue, SettingValue}, ownership};
    use serde_json::json;

    #[test]
    fn native_defaults_preserve_claude_extra_configuration() {
        let mut saved = ownership::default_client_settings(AppKind::Claude);
        saved.settings.insert(
            "spinnerTipsEnabled".into(),
            SettingValue::Explicit { value: ConfigValue::Bool(true) },
        );
        saved.claude_extra.insert("permissions".into(), json!({ "allow": ["git status"] }));

        let (reset, subagents) = ClientConfigurationResetKind::NativeDefaults
            .settings(AppKind::Claude, saved.clone())
            .expect("native reset");

        assert!(reset.settings.values().all(|value| matches!(value, SettingValue::Automatic)));
        assert_eq!(reset.claude_extra, saved.claude_extra);
        assert_eq!(subagents, None);
    }

    #[test]
    fn clearing_extra_configuration_preserves_claude_standard_settings() {
        let mut saved = ownership::default_client_settings(AppKind::Claude);
        saved.settings.insert(
            "spinnerTipsEnabled".into(),
            SettingValue::Explicit { value: ConfigValue::Bool(false) },
        );
        saved.claude_extra.insert("permissions".into(), json!({ "allow": ["git status"] }));

        let (reset, subagents) = ClientConfigurationResetKind::ClearExtraConfiguration
            .settings(AppKind::Claude, saved.clone())
            .expect("clear extra configuration");

        assert_eq!(reset.settings, saved.settings);
        assert!(reset.claude_extra.is_empty());
        assert_eq!(subagents, None);
    }

    #[test]
    fn clearing_extra_configuration_does_not_reproject_standard_settings() {
        let mut saved = ownership::default_client_settings(AppKind::Claude);
        saved.settings.insert(
            "spinnerTipsEnabled".into(),
            SettingValue::Explicit { value: ConfigValue::Bool(false) },
        );
        saved.claude_extra.insert("custom".into(), json!({ "value": "remove" }));
        let (reset, _) = ClientConfigurationResetKind::ClearExtraConfiguration
            .settings(AppKind::Claude, saved)
            .expect("clear extra configuration");
        let current = r#"{
  "spinnerTipsEnabled": true,
  "custom": { "value": "remove" },
  "env": { "ASB_CLAUDE_COMMON_KEYS": "[\"/custom/value\"]" }
}"#;

        let rendered = render_reset_configuration(
            AppKind::Claude,
            current,
            ClientConfigurationResetKind::ClearExtraConfiguration,
            &reset,
            None,
        )
        .expect("render extra clear");

        assert!(rendered.contains("\"spinnerTipsEnabled\": true"));
        assert!(!rendered.contains("\"custom\""));
        assert!(!rendered.contains("ASB_CLAUDE_COMMON_KEYS"));
    }
    #[test]
    fn clearing_extra_configuration_rejects_codex() {
        let error = ClientConfigurationResetKind::ClearExtraConfiguration
            .settings(AppKind::Codex, ownership::default_client_settings(AppKind::Codex))
            .expect_err("Codex has no Claude extra configuration");

        assert_eq!(error.code, "client-configuration-rejected");
    }
}
