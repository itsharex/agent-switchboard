//! Read-only Claude gateway request history commands.

use super::error::{blocking, state, CommandError};
use crate::gateway::request_ledger::{
    ClaudeLedgerFilter, ClaudeRequestLedger, ClaudeRequestLedgerPage, ClaudeRequestLedgerSummary,
};

/// Reads one newest-first page of the durable Claude gateway request ledger.
#[tauri::command]
pub(crate) async fn get_claude_request_ledger(
    app: tauri::AppHandle,
    offset: usize,
    limit: usize,
    filter: Option<ClaudeLedgerFilter>,
) -> Result<ClaudeRequestLedgerPage, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        ClaudeRequestLedger::new(state.claude_request_ledger_path())
            .page(offset, limit, filter.as_ref())
            .map_err(|error| CommandError::new("claude-request-ledger-unavailable", error))
    })
    .await
}

/// Reads credential-free aggregate totals from the Claude gateway ledger.
#[tauri::command]
pub(crate) async fn get_claude_request_ledger_summary(
    app: tauri::AppHandle,
    filter: Option<ClaudeLedgerFilter>,
) -> Result<ClaudeRequestLedgerSummary, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        ClaudeRequestLedger::new(state.claude_request_ledger_path())
            .summary(filter.as_ref())
            .map_err(|error| CommandError::new("claude-request-ledger-unavailable", error))
    })
    .await
}
