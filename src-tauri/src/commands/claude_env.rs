//! Claude 环境变量冲突命令：只读扫描、带备份的删除与恢复。系统环境是用户机器状态，
//! 删除与恢复都要求显式确认；仅 Claude 关心的 `ANTHROPIC*` 前缀。

use super::error::{blocking, require_write_confirmation, state, CommandError};
use crate::claude_env_conflicts::{self, ClaudeEnvBackup, ClaudeEnvScan, ClaudeEnvSelection};
use tauri::AppHandle;

fn failure(message: impl Into<String>) -> CommandError {
    CommandError::new("claude-env-conflict-failed", message)
}

#[tauri::command]
pub(crate) async fn scan_claude_env_conflicts() -> Result<ClaudeEnvScan, CommandError> {
    blocking(|| claude_env_conflicts::scan().map_err(failure)).await
}

#[tauri::command]
pub(crate) async fn remove_claude_env_conflicts(
    app: AppHandle,
    selections: Vec<ClaudeEnvSelection>,
    expected_revision: String,
    confirm_write: bool,
) -> Result<ClaudeEnvBackup, CommandError> {
    require_write_confirmation(confirm_write, "删除 Claude 环境变量")?;
    let local = state(&app)?;
    blocking(move || {
        claude_env_conflicts::remove(local.root(), &selections, &expected_revision).map_err(failure)
    })
    .await
}

#[tauri::command]
pub(crate) async fn list_claude_env_backups(
    app: AppHandle,
) -> Result<Vec<ClaudeEnvBackup>, CommandError> {
    let local = state(&app)?;
    blocking(move || claude_env_conflicts::list_backups(local.root()).map_err(failure)).await
}

#[tauri::command]
pub(crate) async fn restore_claude_env_backup(
    app: AppHandle,
    file_name: String,
    confirm_write: bool,
) -> Result<usize, CommandError> {
    require_write_confirmation(confirm_write, "恢复 Claude 环境变量")?;
    let local = state(&app)?;
    blocking(move || claude_env_conflicts::restore(local.root(), &file_name).map_err(failure)).await
}
