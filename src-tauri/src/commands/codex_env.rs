//! Codex 环境变量冲突命令：只读扫描、带备份的删除与恢复。系统环境是用户机器状态，
//! 删除与恢复都要求显式确认；仅 Codex 关心的 `OPENAI*` 前缀。

use super::error::{blocking, require_write_confirmation, state, CommandError};
use crate::codex_env_conflicts::{self, CodexEnvBackup, CodexEnvScan, CodexEnvSelection};
use tauri::AppHandle;

fn failure(message: impl Into<String>) -> CommandError {
    CommandError::new("codex-env-conflict-failed", message)
}

#[tauri::command]
pub(crate) async fn scan_codex_env_conflicts() -> Result<CodexEnvScan, CommandError> {
    blocking(|| codex_env_conflicts::scan().map_err(failure)).await
}

#[tauri::command]
pub(crate) async fn remove_codex_env_conflicts(
    app: AppHandle,
    selections: Vec<CodexEnvSelection>,
    expected_revision: String,
    confirm_write: bool,
) -> Result<CodexEnvBackup, CommandError> {
    require_write_confirmation(confirm_write, "删除 Codex 环境变量")?;
    let local = state(&app)?;
    blocking(move || {
        codex_env_conflicts::remove(local.root(), &selections, &expected_revision).map_err(failure)
    })
    .await
}

#[tauri::command]
pub(crate) async fn list_codex_env_backups(
    app: AppHandle,
) -> Result<Vec<CodexEnvBackup>, CommandError> {
    let local = state(&app)?;
    blocking(move || codex_env_conflicts::list_backups(local.root()).map_err(failure)).await
}

#[tauri::command]
pub(crate) async fn restore_codex_env_backup(
    app: AppHandle,
    file_name: String,
    confirm_write: bool,
) -> Result<usize, CommandError> {
    require_write_confirmation(confirm_write, "恢复 Codex 环境变量")?;
    let local = state(&app)?;
    blocking(move || codex_env_conflicts::restore(local.root(), &file_name).map_err(failure)).await
}
