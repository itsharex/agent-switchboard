//! Codex sub-agent default-setting commands.
//!
//! The renderer sends only validated settings and the hashes it read; this
//! module resolves the real user-level `config.toml` path and delegates every
//! mutation to the `asb-switch` transaction. There is no application-side
//! copy of these three runtime keys and no supplier profile involvement.

use super::error::{blocking, observe, require_write_confirmation, state, CommandError};
use super::ConfigWriteGate;
use crate::runtime_log::RuntimeLogAction;
use asb_core::{
    CodexSubagentSettings, CodexSubagentSettingsPreview, CodexSubagentSettingsSnapshot,
    SubagentSettingsPlan,
};
use asb_switch::{
    preview_codex_subagent_settings, read_codex_subagent_settings, recovery_label,
    write_codex_subagent_settings, SubagentSettingsRequest,
};
use tauri::{AppHandle, Manager};

fn subagent_error(error: asb_switch::SwitchError) -> CommandError {
    match &error {
        asb_switch::SwitchError::ReadCurrent { .. } => {
            CommandError::new("subagent-settings-unreadable", "无法读取用户级 Codex 配置")
        }
        asb_switch::SwitchError::PlanRejected { message, .. } => {
            CommandError::new("subagent-settings-rejected", message.clone())
        }
        asb_switch::SwitchError::BlockedByLock { .. } => CommandError::new(
            "subagent-settings-locked",
            "用户级 Codex 配置正被其他写入操作占用",
        ),
        asb_switch::SwitchError::ExternalChange { .. } => CommandError::new(
            "subagent-settings-external-change",
            "用户级 Codex 配置已在读取后被外部修改，请重新读取后再操作",
        ),
        asb_switch::SwitchError::PlanChanged => CommandError::new(
            "subagent-settings-preview-stale",
            "候选配置已与预览不一致，请重新生成预览",
        ),
        asb_switch::SwitchError::CommitFailed { recovery, .. } => CommandError::new(
            "subagent-settings-commit-failed",
            format!("应用子 agent 设置失败；{}", recovery_label(recovery)),
        ),
        asb_switch::SwitchError::LockReleaseFailed { prior, .. } => {
            CommandError::new("subagent-settings-lock-release-failed", prior.to_string())
        }
    }
}

/// Reads the three canonical global sub-agent scalars from the real user-level
/// configuration. The backend never returns the absolute path.
#[tauri::command]
pub async fn get_codex_subagent_settings(
    app: AppHandle,
) -> Result<CodexSubagentSettingsSnapshot, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let target = state
            .target(asb_core::AppKind::Codex)
            .map_err(|error| CommandError::new("subagent-settings-path-unavailable", error))?;
        read_codex_subagent_settings(&asb_switch::FsIo, &target).map_err(subagent_error)
    })
    .await
}

/// Renders the complete redacted candidate document for the current draft.
/// The baseline hash must still match the real file; otherwise the caller is
/// told to re-read instead of being shown a merged guess.
#[tauri::command]
pub async fn preview_codex_subagent_settings_command(
    app: AppHandle,
    settings: CodexSubagentSettings,
    expected_hash: String,
) -> Result<CodexSubagentSettingsPreview, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let target = state
            .target(asb_core::AppKind::Codex)
            .map_err(|error| CommandError::new("subagent-settings-path-unavailable", error))?;
        settings
            .validate()
            .map_err(|error| CommandError::new("subagent-settings-rejected", error.to_string()))?;
        let current = match std::fs::read_to_string(&target) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(CommandError::new(
                    "subagent-settings-unreadable",
                    error.to_string(),
                ))
            }
        };
        let found = asb_switch::sha256_hex(&current);
        if found != expected_hash {
            return Err(CommandError::new(
                "subagent-settings-external-change",
                "用户级 Codex 配置已在读取后被外部修改，请重新读取后再操作",
            ));
        }
        preview_codex_subagent_settings(&current, &settings).map_err(subagent_error)
    })
    .await
}

/// Applies one confirmed sub-agent settings plan through the executor
/// transaction. This touches only the three global runtime scalars; supplier
/// profiles, role tables, and unrelated host configuration are untouched.
#[tauri::command]
pub async fn apply_codex_subagent_settings(
    app: AppHandle,
    plan: SubagentSettingsPlan,
    confirm_write: bool,
) -> Result<CodexSubagentSettingsSnapshot, CommandError> {
    observe(RuntimeLogAction::CodexSubagentSettingsApplied, async move {
        require_write_confirmation(confirm_write, "应用子 agent 设置")?;
        let state = state(&app)?;
        let write_gate: ConfigWriteGate = app
            .try_state::<ConfigWriteGate>()
            .ok_or_else(|| CommandError::new("app-state-unavailable", "写入闸门尚未初始化"))?
            .inner()
            .clone();
        blocking(move || {
            let _write_gate = write_gate
                .lock()
                .map_err(|error| CommandError::new("config-write-gate-unavailable", error))?;
            let target = state
                .target(asb_core::AppKind::Codex)
                .map_err(|error| CommandError::new("subagent-settings-path-unavailable", error))?;
            let backup_dir = state.backup_dir();
            write_codex_subagent_settings(
                &asb_switch::FsIo,
                &SubagentSettingsRequest {
                    target: &target,
                    backup_dir: &backup_dir,
                    settings: &plan.settings,
                    expected_hash: &plan.expected_hash,
                    expected_target_existed: plan.expected_target_existed,
                    expected_rendered_hash: &plan.rendered_hash,
                },
            )
            .map_err(subagent_error)
        })
        .await
    })
    .await
}
