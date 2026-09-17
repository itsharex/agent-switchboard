//! Confirmed manual edits for client configuration outside the typed UI fields.
//!
//! The renderer edits a redacted display copy. This command rehydrates unchanged
//! secret markers on the backend, reapplies the current typed overlay, then
//! uses the same backup and recovery executor as every other client write.

use super::{
    client_configuration_apply::{
        commit_rendered_client_configuration, render_client_configuration, ClientConfigurationApplyPreview,
    },
    error::{blocking, require_write_confirmation, state, store_error, CommandError},
    ConfigWriteGate,
};
use asb_core::contracts::{AppKind, SettingsValues};
use asb_switch::{preview_rendered, rehydrate_display_content, sha256_hex};
use std::io::ErrorKind;
use tauri::{AppHandle, Manager};

fn candidate(
    state: &crate::local_state::LocalState,
    target: AppKind,
    expected_source_hash: &str,
    display_content: &str,
    settings: SettingsValues,
    subagent_settings: Option<asb_core::CodexSubagentSettings>,
) -> Result<(String, bool, SettingsValues, String), CommandError> {
    let path = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let current = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(_) => return Err(CommandError::new("client-configuration-unreadable", "无法读取真实客户端配置文件")),
    };
    if sha256_hex(&current) != expected_source_hash {
        return Err(CommandError::new("client-configuration-preview-stale", "真实配置已变化，请重新读取后再编辑"));
    }
    let source = if current.is_empty() && target == AppKind::Claude { "{}" } else { &current };
    if display_content == asb_switch::display_content(target, source) {
        return Err(CommandError::new("manual-configuration-no-change", "手动配置没有变更"));
    }
    let manual = rehydrate_display_content(target, &current, display_content).map_err(CommandError::from)?;
    let baseline = source;
    let manually_changed_owned = asb_core::adapter::owned_diff(target, &manual, baseline)
        .map_err(|error| CommandError::new("manual-configuration-rejected", error.message))?;
    if !manually_changed_owned.is_empty() {
        let paths = manually_changed_owned.into_iter().map(|change| change.key).collect::<Vec<_>>().join("、");
        return Err(CommandError::new(
            "manual-configuration-rejected",
            format!("手动配置只能修改界面未拥有的字段；请在对应界面修改：{paths}"),
        ));
    }
    let rendered = render_client_configuration(target, &manual, &settings, subagent_settings.as_ref())?;
    Ok((current, path.exists(), settings, rendered))
}

#[tauri::command]
pub async fn preview_manual_client_configuration(
    app: AppHandle,
    target: AppKind,
    expected_source_hash: String,
    display_content: String,
    settings: SettingsValues,
    subagent_settings: Option<asb_core::CodexSubagentSettings>,
) -> Result<ClientConfigurationApplyPreview, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let stored = state.configuration().get_client_settings(target).map_err(store_error)?;
        let (current, existed, _, rendered) = candidate(
            &state,
            target,
            &expected_source_hash,
            &display_content,
            settings,
            subagent_settings,
        )?;
        let path = state.target(target).map_err(|error| CommandError::new("config-path-unavailable", error))?;
        let file = preview_rendered(target, &path, &state.backup_dir(), &current, &rendered)
            .map_err(CommandError::from)?;
        Ok(ClientConfigurationApplyPreview {
            file,
            settings_hash: stored.settings_hash,
            target_existed: existed,
        })
    }).await
}

#[tauri::command]
pub async fn commit_manual_client_configuration(
    app: AppHandle,
    target: AppKind,
    expected_source_hash: String,
    expected_rendered_hash: String,
    expected_settings_hash: String,
    expected_target_existed: bool,
    display_content: String,
    settings: SettingsValues,
    subagent_settings: Option<asb_core::CodexSubagentSettings>,
    confirm_write: bool,
) -> Result<(), CommandError> {
    require_write_confirmation(confirm_write, "应用手动客户端配置")?;
    let state = state(&app)?;
    let gate = app.try_state::<ConfigWriteGate>()
        .ok_or_else(|| CommandError::new("app-state-unavailable", "写入闸门尚未初始化"))?
        .inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(|error| CommandError::new("config-write-gate-unavailable", error))?;
        crate::commands::switching::ensure_profile_save_recovered(&app)?;
        let (current, existed, settings, rendered) = candidate(
            &state,
            target,
            &expected_source_hash,
            &display_content,
            settings,
            subagent_settings,
        )?;
        commit_rendered_client_configuration(
            &state,
            target,
            &current,
            existed,
            settings,
            &rendered,
            &expected_source_hash,
            &expected_rendered_hash,
            &expected_settings_hash,
            expected_target_existed,
            "manual-client-configuration",
        )
    }).await
}
