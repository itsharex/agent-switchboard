//! Claude prompt library commands; the native Markdown editor remains unchanged.

use super::{
    error::{blocking, observe, require_write_confirmation, state, CommandError},
    ConfigWriteGate,
};
use crate::claude_prompts::{self, contracts::*};
use crate::runtime_log::RuntimeLogAction;
use tauri::{AppHandle, Manager};

fn error(message: String) -> CommandError {
    CommandError::new("claude-prompt-operation-failed", message)
}

#[tauri::command]
pub(crate) async fn list_claude_prompts(app: AppHandle) -> Result<ClaudePromptsView, CommandError> {
    let local = state(&app)?;
    blocking(move || {
        claude_prompts::list(
            local.root(),
            &local
                .global_prompt_target(asb_core::AppKind::Claude)
                .map_err(error)?,
        )
        .map_err(error)
    })
    .await
}

#[tauri::command]
pub(crate) async fn save_claude_prompt(
    app: AppHandle,
    prompt_id: Option<String>,
    draft: ClaudePromptDraft,
    expected_file_hash: String,
    confirm_write: bool,
) -> Result<ClaudePromptsView, CommandError> {
    require_write_confirmation(confirm_write, "保存 Claude 提示词预设")?;
    let local = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        let target = local
            .global_prompt_target(asb_core::AppKind::Claude)
            .map_err(error)?;
        claude_prompts::save(
            local.root(),
            &target,
            prompt_id.as_deref(),
            draft,
            &expected_file_hash,
        )
        .map_err(error)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remove_claude_prompt(
    app: AppHandle,
    prompt_id: String,
    expected_file_hash: String,
    confirm_write: bool,
) -> Result<ClaudePromptsView, CommandError> {
    require_write_confirmation(confirm_write, "删除 Claude 提示词预设")?;
    let local = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        let target = local
            .global_prompt_target(asb_core::AppKind::Claude)
            .map_err(error)?;
        claude_prompts::remove(local.root(), &target, &prompt_id, &expected_file_hash)
            .map_err(error)
    })
    .await
}

#[tauri::command]
pub(crate) async fn reorder_claude_prompts(
    app: AppHandle,
    ordered_ids: Vec<String>,
    expected_file_hash: String,
    confirm_write: bool,
) -> Result<ClaudePromptsView, CommandError> {
    require_write_confirmation(confirm_write, "调整 Claude 提示词顺序")?;
    let local = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        let target = local
            .global_prompt_target(asb_core::AppKind::Claude)
            .map_err(error)?;
        claude_prompts::reorder(local.root(), &target, &ordered_ids, &expected_file_hash)
            .map_err(error)
    })
    .await
}

#[tauri::command]
pub(crate) async fn preview_claude_prompt(
    app: AppHandle,
    prompt_id: Option<String>,
    expected_file_hash: String,
) -> Result<ClaudePromptPreview, CommandError> {
    let local = state(&app)?;
    blocking(move || {
        claude_prompts::preview(
            local.root(),
            &local
                .global_prompt_target(asb_core::AppKind::Claude)
                .map_err(error)?,
            prompt_id,
            &expected_file_hash,
        )
        .map_err(error)
    })
    .await
}

#[tauri::command]
pub(crate) async fn activate_claude_prompt(
    app: AppHandle,
    plan: ClaudePromptActivation,
    confirm_write: bool,
) -> Result<ClaudePromptsView, CommandError> {
    require_write_confirmation(confirm_write, "应用 Claude 提示词到 CLAUDE.md")?;
    let local = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    observe(
        RuntimeLogAction::GlobalPromptDocumentSaved,
        blocking(move || {
            let _guard = gate.lock().map_err(error)?;
            let target = local
                .global_prompt_target(asb_core::AppKind::Claude)
                .map_err(error)?;
            claude_prompts::activate(local.root(), &target, &local.prompt_backup_dir(), plan)
                .map_err(error)
        }),
    )
    .await
}

#[tauri::command]
pub(crate) async fn recover_claude_prompt(
    app: AppHandle,
    confirm_write: bool,
) -> Result<ClaudePromptsView, CommandError> {
    require_write_confirmation(confirm_write, "恢复 Claude 提示词事务")?;
    let local = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        claude_prompts::recover(
            local.root(),
            &local
                .global_prompt_target(asb_core::AppKind::Claude)
                .map_err(error)?,
        )
        .map_err(error)
    })
    .await
}

#[tauri::command]
pub(crate) async fn scan_claude_prompt_source(
    source_path: String,
) -> Result<claude_prompts::source::ClaudePromptSource, CommandError> {
    blocking(move || {
        claude_prompts::source::scan(std::path::Path::new(&source_path)).map_err(error)
    })
    .await
}

#[tauri::command]
pub(crate) async fn import_claude_prompt_source(
    app: AppHandle,
    source_path: String,
    source_ids: Vec<String>,
    source_revision: String,
    expected_file_hash: String,
    confirm_write: bool,
) -> Result<claude_prompts::source::ClaudePromptImportResult, CommandError> {
    require_write_confirmation(confirm_write, "导入 Claude 提示词预设")?;
    let local = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        let target = local
            .global_prompt_target(asb_core::AppKind::Claude)
            .map_err(error)?;
        claude_prompts::source::import(
            local.root(),
            &target,
            std::path::Path::new(&source_path),
            &source_ids,
            &source_revision,
            &expected_file_hash,
        )
        .map_err(error)
    })
    .await
}
