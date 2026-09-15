//! Claude-only import of the CC Switch failover queue and proxy policy.
//! The scan is read-only; the confirmed import writes only this application's
//! own Claude policy file through the shared validate/save/refresh path.
//! Codex keeps no queue here, so nothing in this module can touch Codex state.

use super::error::{blocking, observe, require_write_confirmation, state, CommandError};
use super::failover::{self, ClaudeFailoverView};
use super::ConfigWriteGate;
use crate::ccswitch_source::claude_failover as source;
use crate::gateway::GatewayController;
use crate::runtime_log::RuntimeLogAction;
use serde::Serialize;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeFailoverImportOutcome {
    pub view: ClaudeFailoverView,
    pub queued: usize,
    pub warnings: Vec<String>,
}

fn gate(app: &AppHandle) -> Result<ConfigWriteGate, CommandError> {
    app.try_state::<ConfigWriteGate>()
        .map(|gate| gate.inner().clone())
        .ok_or_else(|| CommandError::new("claude-failover-gate-unavailable", "写入闸门尚未初始化"))
}

fn failure(message: impl Into<String>) -> CommandError {
    CommandError::new("claude-failover-import-failed", message)
}

#[tauri::command]
pub(crate) async fn scan_claude_failover_source(
    app: AppHandle,
    source_path: String,
) -> Result<source::ClaudeFailoverSourceScan, CommandError> {
    let local = state(&app)?;
    blocking(move || source::scan(std::path::Path::new(&source_path), &local).map_err(failure))
        .await
}

#[tauri::command]
pub(crate) async fn import_claude_failover_source(
    app: AppHandle,
    source_path: String,
    source_revision: String,
    expected_policy_hash: String,
    confirm_write: bool,
) -> Result<ClaudeFailoverImportOutcome, CommandError> {
    require_write_confirmation(confirm_write, "导入 Claude 故障转移队列")?;
    let gate = gate(&app)?;
    observe(RuntimeLogAction::ProfileUpdated, async move {
        let state = state(&app)?;
        let gateway = app.state::<GatewayController>().inner().clone();
        blocking(move || {
            let _guard = gate
                .lock()
                .map_err(|error| CommandError::new("claude-failover-gate-unavailable", error))?;
            let (policy, warnings) = source::import(
                std::path::Path::new(&source_path),
                &source_revision,
                &expected_policy_hash,
                &state,
            )
            .map_err(failure)?;
            let previous = failover::read_policy(&state)?;
            let validated = failover::validate_policy(&state, &policy)?;
            failover::save_and_refresh(&state, &gateway, &previous, &validated)?;
            Ok(ClaudeFailoverImportOutcome {
                view: failover::view(&state, &gateway)?,
                queued: validated.provider_ids.len(),
                warnings,
            })
        })
        .await
    })
    .await
}
