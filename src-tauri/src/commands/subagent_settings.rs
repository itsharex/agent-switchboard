//! Read-only Codex sub-agent runtime state for the unified client-configuration page.
use super::error::{blocking, state, CommandError};
use asb_core::CodexSubagentSettingsSnapshot;
use asb_switch::read_codex_subagent_settings;
use tauri::AppHandle;

#[tauri::command]
pub async fn get_codex_subagent_settings(
    app: AppHandle,
) -> Result<CodexSubagentSettingsSnapshot, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let target = state
            .target(asb_core::AppKind::Codex)
            .map_err(|error| CommandError::new("subagent-settings-path-unavailable", error))?;
        read_codex_subagent_settings(&asb_switch::FsIo, &target)
            .map_err(|_| CommandError::keyed("subagent-settings-unreadable", "errors.cfg.subagentSettingsUnreadable", "无法读取用户级 Codex 配置"))
    })
    .await
}
