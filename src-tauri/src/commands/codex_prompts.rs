use super::{
    error::{blocking, require_write_confirmation, state, CommandError},
    ConfigWriteGate,
};
use crate::codex_prompts::{self, contracts::*};
use tauri::{AppHandle, Manager};
fn error(message: String) -> CommandError {
    CommandError::new("codex-prompt-operation-failed", message)
}
#[tauri::command]
pub(crate) async fn list_codex_prompts(app: AppHandle) -> Result<PromptsView, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        codex_prompts::list(
            state.root(),
            &state
                .global_prompt_target(asb_core::AppKind::Codex)
                .map_err(error)?,
        )
        .map_err(error)
    })
    .await
}
#[tauri::command]
pub(crate) async fn save_codex_prompt(
    app: AppHandle,
    preset_id: Option<String>,
    draft: PromptDraft,
    expected_revision: String,
    expected_live_hash: Option<String>,
    confirm_write: bool,
) -> Result<PromptsView, CommandError> {
    let state = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        let target = state
            .global_prompt_target(asb_core::AppKind::Codex)
            .map_err(error)?;
        codex_prompts::save(
            state.root(),
            &target,
            &state.prompt_backup_dir(),
            preset_id.as_deref(),
            draft,
            &expected_revision,
            expected_live_hash.as_deref(),
            confirm_write,
        )
        .map_err(error)
    })
    .await
}
#[tauri::command]
pub(crate) async fn delete_codex_prompt(
    app: AppHandle,
    preset_id: String,
    expected_revision: String,
    confirm_write: bool,
) -> Result<PromptsView, CommandError> {
    require_write_confirmation(confirm_write, "删除 Codex 指令预设")?;
    let state = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        codex_prompts::delete(
            state.root(),
            &state
                .global_prompt_target(asb_core::AppKind::Codex)
                .map_err(error)?,
            &preset_id,
            &expected_revision,
        )
        .map_err(error)
    })
    .await
}
#[tauri::command]
pub(crate) async fn preview_codex_prompt(
    app: AppHandle,
    preset_id: Option<String>,
    expected_revision: String,
) -> Result<PromptPreview, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        codex_prompts::preview(
            state.root(),
            &state
                .global_prompt_target(asb_core::AppKind::Codex)
                .map_err(error)?,
            preset_id,
            &expected_revision,
        )
        .map_err(error)
    })
    .await
}
#[tauri::command]
pub(crate) async fn apply_codex_prompt(
    app: AppHandle,
    plan: PromptActivation,
    confirm_write: bool,
) -> Result<PromptsView, CommandError> {
    require_write_confirmation(confirm_write, "应用 Codex 指令预设")?;
    let state = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    super::error::observe(
        crate::runtime_log::RuntimeLogAction::GlobalPromptDocumentSaved,
        blocking(move || {
            let _guard = gate.lock().map_err(error)?;
            let target = state
                .global_prompt_target(asb_core::AppKind::Codex)
                .map_err(error)?;
            codex_prompts::activate(state.root(), &target, &state.prompt_backup_dir(), plan)
                .map_err(error)
        }),
    )
    .await
}
#[tauri::command]
pub(crate) async fn recover_codex_prompt(
    app: AppHandle,
    confirm_write: bool,
) -> Result<PromptsView, CommandError> {
    require_write_confirmation(confirm_write, "恢复 Codex 指令事务")?;
    let state = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        codex_prompts::recover(
            state.root(),
            &state
                .global_prompt_target(asb_core::AppKind::Codex)
                .map_err(error)?,
        )
        .map_err(error)
    })
    .await
}
