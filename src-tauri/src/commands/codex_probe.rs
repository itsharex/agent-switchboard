//! Codex degradation probe commands: the question catalog plus the
//! start/poll/cancel trio around one batch run. Every run spends real Codex
//! quota and stays visible in the usage report.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::codex_probe::{self, ProbeRegistry, ProbeStatus, MAX_RUN_COUNT};
use crate::commands::error::CommandError;

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexProbeStartedDto {
    probe_id: String,
}

#[tauri::command]
pub async fn start_codex_probe(
    app: AppHandle,
    request: StartCodexProbeRequest,
) -> Result<CodexProbeStartedDto, CommandError> {
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
    let registry = app.state::<ProbeRegistry>().inner().clone();
    let probe_id = codex_probe::spawn_probe(&registry, question, request.run_count)
        .map_err(|message| CommandError::new("codex-probe-busy", message))?;
    Ok(CodexProbeStartedDto { probe_id })
}

#[tauri::command]
pub async fn get_codex_probe(
    app: AppHandle,
    probe_id: String,
) -> Result<Option<ProbeStatus>, CommandError> {
    Ok(app.state::<ProbeRegistry>().status(&probe_id))
}

#[tauri::command]
pub async fn cancel_codex_probe(app: AppHandle, probe_id: String) -> Result<bool, CommandError> {
    Ok(app.state::<ProbeRegistry>().cancel(&probe_id))
}
