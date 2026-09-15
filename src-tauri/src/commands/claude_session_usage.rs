//! Claude CLI 会话用量命令（Claude 专属；Codex 会话同步在 codex_metering）。

use super::error::{blocking, require_write_confirmation, state, CommandError};
use crate::claude_session_usage::{self, ClaudeSessionSyncReport, ClaudeSessionUsageSummary};
use serde::Serialize;
use tauri::AppHandle;

fn failure(message: String) -> CommandError {
    CommandError::new("claude-session-usage-unavailable", message)
}

/// Syncs incrementally, then reports totals; per-file problems ride along in
/// the report so a partial scan is never mistaken for a complete one.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeSessionUsageView {
    pub report: ClaudeSessionSyncReport,
    pub summary: ClaudeSessionUsageSummary,
}

#[tauri::command]
pub(crate) async fn get_claude_session_usage(
    app: AppHandle,
) -> Result<ClaudeSessionUsageView, CommandError> {
    let local = state(&app)?;
    blocking(move || {
        let report = claude_session_usage::sync(local.root());
        let summary = claude_session_usage::summary(local.root()).map_err(failure)?;
        Ok(ClaudeSessionUsageView { report, summary })
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeSessionRebuildOutcome {
    pub backup_file: Option<String>,
    pub report: ClaudeSessionSyncReport,
    pub summary: ClaudeSessionUsageSummary,
}

#[tauri::command]
pub(crate) async fn rebuild_claude_session_usage(
    app: AppHandle,
    confirm_write: bool,
) -> Result<ClaudeSessionRebuildOutcome, CommandError> {
    require_write_confirmation(confirm_write, "重建 Claude 会话用量账本")?;
    let local = state(&app)?;
    blocking(move || {
        let (backup_file, report) = claude_session_usage::rebuild(local.root()).map_err(failure)?;
        let summary = claude_session_usage::summary(local.root()).map_err(failure)?;
        Ok(ClaudeSessionRebuildOutcome {
            backup_file,
            report,
            summary,
        })
    })
    .await
}
