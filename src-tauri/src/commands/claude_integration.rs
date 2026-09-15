//! Claude 客户端集成标记命令：插件 `primaryApiKey` 与引导跳过标记的读取、预览、
//! 确认写入与切换后同步策略。只涉及 Claude 客户端文件，Codex 没有对应物。

use super::error::{blocking, require_write_confirmation, state, CommandError};
use crate::claude_integration::{
    self, ClaudeIntegrationFlag, ClaudeIntegrationPolicy, ClaudeIntegrationPreview,
    ClaudeIntegrationView,
};
use tauri::{AppHandle, Manager};

fn failure(message: impl Into<String>) -> CommandError {
    CommandError::new("claude-integration-failed", message)
}

#[tauri::command]
pub(crate) async fn get_claude_integration(
    app: AppHandle,
) -> Result<ClaudeIntegrationView, CommandError> {
    let local = state(&app)?;
    blocking(move || claude_integration::view(local.root()).map_err(failure)).await
}

#[tauri::command]
pub(crate) async fn set_claude_integration_policy(
    app: AppHandle,
    policy: ClaudeIntegrationPolicy,
    confirm_write: bool,
) -> Result<ClaudeIntegrationView, CommandError> {
    require_write_confirmation(confirm_write, "保存 Claude 客户端集成策略")?;
    let local = state(&app)?;
    blocking(move || claude_integration::save_policy(local.root(), &policy).map_err(failure)).await
}

#[tauri::command]
pub(crate) async fn preview_claude_integration(
    flag: ClaudeIntegrationFlag,
    enable: bool,
) -> Result<ClaudeIntegrationPreview, CommandError> {
    blocking(move || {
        claude_integration::preview(flag, enable)
            .map(|(plan, _)| plan)
            .map_err(failure)
    })
    .await
}

#[tauri::command]
pub(crate) async fn apply_claude_integration(
    app: AppHandle,
    preview: ClaudeIntegrationPreview,
    confirm_write: bool,
) -> Result<ClaudeIntegrationView, CommandError> {
    require_write_confirmation(confirm_write, "写入 Claude 客户端集成标记")?;
    let local = state(&app)?;
    blocking(move || {
        let gate = app
            .try_state::<super::ConfigWriteGate>()
            .map(|gate| gate.inner().clone())
            .ok_or_else(|| CommandError::new("app-state-unavailable", "写入闸门尚未初始化"))?;
        let _gate = gate
            .lock()
            .map_err(|error| CommandError::new("config-write-gate-unavailable", error))?;
        claude_integration::apply(local.root(), &preview).map_err(failure)
    })
    .await
}
