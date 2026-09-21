//! Codex degradation probe commands: the question catalog, the single-batch
//! start/cancel/retry trio, and the persisted history reads and deletes.
//! Every run spends real Codex quota and is recorded in the local probe
//! history before, during, and after the calls.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::codex_probe::{
    self, ProbeBatchStatus, ProbeBatchView, ProbeHistoryPage, ProbeHistoryProfile,
    ProbeProfileFilter, ProbeRegistry, MAX_RUN_COUNT,
};
use crate::commands::error::{blocking, state, CommandError};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexProbeQuestionDto<'a> {
    id: &'a str,
    label: &'a str,
}

#[tauri::command]
pub async fn list_codex_probe_questions() -> Result<Vec<CodexProbeQuestionDto<'static>>, CommandError> {
    Ok(codex_probe::QUESTIONS
        .iter()
        .map(|question| CodexProbeQuestionDto {
            id: question.id,
            label: question.label,
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartCodexProbeRequest {
    run_count: u32,
    /// A built-in question id, or `custom` to use the user-authored question.
    question_id: String,
    custom_question: Option<String>,
    custom_answer: Option<String>,
}

#[tauri::command]
pub async fn start_codex_probe(
    app: AppHandle,
    request: StartCodexProbeRequest,
) -> Result<(), CommandError> {
    if request.run_count == 0 || request.run_count > MAX_RUN_COUNT {
        return Err(CommandError::new(
            "codex-probe-invalid-count",
            format!("检测次数必须在 1 到 {MAX_RUN_COUNT} 之间"),
        ));
    }
    let question = codex_probe::resolve_question(
        &request.question_id,
        request.custom_question,
        request.custom_answer,
    )
    .map_err(|message| CommandError::new("codex-probe-invalid-question", message))?;
    let local = state(&app)?;
    let registry = app.state::<ProbeRegistry>().inner().clone();
    let gateway = app
        .try_state::<crate::gateway::GatewayController>()
        .map(|controller| controller.inner().clone());
    blocking(move || {
        codex_probe::spawn_probe(&registry, &local, question, request.run_count, gateway.as_ref())
            .map(|_| ())
            .map_err(|message| CommandError::new("codex-probe-start-failed", message))
    })
    .await
}

/// The live-or-latest batch; the panel reconnects here after a refresh or
/// restart without remembering any id.
#[tauri::command]
pub async fn get_current_codex_probe(app: AppHandle) -> Result<Option<ProbeBatchView>, CommandError> {
    let local = state(&app)?;
    let registry = app.state::<ProbeRegistry>().inner().clone();
    blocking(move || {
        codex_probe::current_view(&local, &registry)
            .map_err(|message| CommandError::new("codex-probe-read-failed", message))
    })
    .await
}

#[tauri::command]
pub async fn cancel_codex_probe(app: AppHandle) -> Result<bool, CommandError> {
    Ok(app.state::<ProbeRegistry>().cancel())
}

/// Flushes results whose persistence failed mid-batch.
#[tauri::command]
pub async fn retry_codex_probe_save(app: AppHandle) -> Result<(), CommandError> {
    let local = state(&app)?;
    let registry = app.state::<ProbeRegistry>().inner().clone();
    blocking(move || {
        codex_probe::retry_save(&local, &registry)
            .map_err(|message| CommandError::new("codex-probe-save-failed", message))
    })
    .await
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum CodexProbeProfileFilterDto {
    All {},
    Unlinked {},
    Profile { id: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListCodexProbeHistoryRequest {
    offset: u32,
    limit: u32,
    /// Omitted (or zero) means all time.
    days: Option<u32>,
    profile: Option<CodexProbeProfileFilterDto>,
    /// Omitted means every status.
    status: Option<String>,
}

#[tauri::command]
pub async fn list_codex_probe_history(
    app: AppHandle,
    request: ListCodexProbeHistoryRequest,
) -> Result<ProbeHistoryPage, CommandError> {
    let status = match request.status.as_deref() {
        None | Some("all") => None,
        Some(value) => Some(ProbeBatchStatus::parse(value).ok_or_else(|| {
            CommandError::new("codex-probe-history-invalid", format!("未知的检测状态：{value}"))
        })?),
    };
    let profile = match request.profile {
        None | Some(CodexProbeProfileFilterDto::All {}) => ProbeProfileFilter::All,
        Some(CodexProbeProfileFilterDto::Unlinked {}) => ProbeProfileFilter::Unlinked,
        Some(CodexProbeProfileFilterDto::Profile { id }) => ProbeProfileFilter::Id(id),
    };
    let local = state(&app)?;
    blocking(move || {
        let ledger = codex_probe::ProbeLedger::new(local.root());
        ledger
            .list_history(&codex_probe::ProbeHistoryQuery {
                offset: request.offset,
                limit: request.limit,
                days: request.days.filter(|days| *days > 0),
                profile,
                status,
            })
            .map_err(|message| CommandError::new("codex-probe-history-failed", message))
    })
    .await
}

#[tauri::command]
pub async fn list_codex_probe_history_profiles(
    app: AppHandle,
) -> Result<Vec<ProbeHistoryProfile>, CommandError> {
    let local = state(&app)?;
    blocking(move || {
        codex_probe::ProbeLedger::new(local.root())
            .list_history_profiles()
            .map_err(|message| CommandError::new("codex-probe-history-failed", message))
    })
    .await
}

#[tauri::command]
pub async fn get_codex_probe_batch(
    app: AppHandle,
    batch_id: String,
) -> Result<Option<ProbeBatchView>, CommandError> {
    let local = state(&app)?;
    let registry = app.state::<ProbeRegistry>().inner().clone();
    blocking(move || {
        codex_probe::batch_view(&local, &registry, &batch_id)
            .map_err(|message| CommandError::new("codex-probe-history-failed", message))
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteCodexProbeBatchesRequest {
    batch_ids: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedCodexProbeBatchesDto {
    deleted: u32,
}

/// Deletes finished batches and their runs — only the radar's own records;
/// Codex sessions and the real usage ledger stay untouched.
#[tauri::command]
pub async fn delete_codex_probe_batches(
    app: AppHandle,
    request: DeleteCodexProbeBatchesRequest,
) -> Result<DeletedCodexProbeBatchesDto, CommandError> {
    let local = state(&app)?;
    blocking(move || {
        codex_probe::ProbeLedger::new(local.root())
            .delete_batches(&request.batch_ids)
            .map(|deleted| DeletedCodexProbeBatchesDto { deleted })
            .map_err(|message| CommandError::new("codex-probe-delete-failed", message))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_status_filter_rejects_unknown_values() {
        assert!(ProbeBatchStatus::parse("completed").is_some());
        assert!(ProbeBatchStatus::parse("config-changed").is_some());
        assert!(ProbeBatchStatus::parse("expired").is_none());
    }
}
