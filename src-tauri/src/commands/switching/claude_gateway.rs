//! Claude-only gateway shutdown through the existing recoverable file executor.

use super::*;
use crate::gateway::GatewayController;
use crate::local_state::LocalState;
use asb_core::contracts::ConfigWriteRecord;
use asb_switch::{execute_rendered, RenderedWriteOutcome, RenderedWriteRequest};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeGatewayStopPreview {
    pub(crate) backup_id: String,
    pub(crate) content_hash: String,
    pub(crate) rendered_hash: String,
    pub(crate) target: String,
    pub(crate) changes: Vec<KeyChange>,
}

fn failure(message: impl Into<String>) -> CommandError {
    CommandError::new("claude-gateway-restore-failed", message)
}

fn candidate(
    local: &LocalState,
    gateway: &GatewayController,
    backup_id: Option<&str>,
) -> Result<(ClaudeGatewayStopPreview, String), CommandError> {
    let target = local.target(AppKind::Claude).map_err(failure)?;
    let current =
        fs::read_to_string(&target).map_err(|e| failure(format!("无法读取 Claude 配置：{e}")))?;
    if gateway
        .active_profile_id(AppKind::Claude, &current)
        .map_err(failure)?
        .is_none()
    {
        return Err(failure("当前 Claude 配置已不属于活动接管路由；未修改任何文件，请先重新应用供应商或在切换历史中恢复"));
    }
    let mut records = local_backups(local)?;
    records.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.id.cmp(&left.id))
    });
    for record in records.into_iter().filter(|record| {
        record.app == AppKind::Claude && backup_id.is_none_or(|id| record.id == id)
    }) {
        let before = fs::read_to_string(&record.backup_path)
            .map_err(|e| failure(format!("接管前备份不可读：{e}")))?;
        if sha256_hex(&before) != record.content_hash {
            return Err(failure("接管前备份校验失败，已拒绝恢复"));
        }
        let before = if before.is_empty() && !record.target_existed {
            "{}"
        } else {
            &before
        };
        adapter::validate_syntax(AppKind::Claude, before).map_err(|e| failure(e.to_string()))?;
        if crate::gateway::claude_uses_gateway(before) {
            continue;
        }
        let rendered = adapter::claude::restore_gateway_overlay(&current, before)
            .map_err(|e| failure(e.to_string()))?;
        let changes = adapter::owned_diff(AppKind::Claude, &current, &rendered)
            .map_err(|e| failure(e.to_string()))?;
        return Ok((
            ClaudeGatewayStopPreview {
                backup_id: record.id,
                content_hash: sha256_hex(&current),
                rendered_hash: sha256_hex(&rendered),
                target: target.to_string_lossy().into(),
                changes,
            },
            rendered,
        ));
    }
    Err(failure(
        "找不到接管前的 Claude 直连/官方配置备份；请在供应商页明确切换到官方登录或直连供应商",
    ))
}

pub(crate) fn stop(
    local: &LocalState,
    gateway: &GatewayController,
    preview: &ClaudeGatewayStopPreview,
) -> Result<RenderedWriteOutcome, CommandError> {
    let (current, rendered) = candidate(local, gateway, Some(&preview.backup_id))?;
    if current.content_hash != preview.content_hash
        || current.rendered_hash != preview.rendered_hash
        || current.target != preview.target
    {
        return Err(failure("Claude 配置或备份在预览后改变，请重新预览"));
    }
    let target = PathBuf::from(&current.target);
    let backup_dir = local.backup_dir();
    transaction::begin(
        local,
        gateway,
        AppKind::Claude,
        None,
        &current.rendered_hash,
        true,
        None,
    )?;
    let result = execute_rendered(
        &FsIo,
        &RenderedWriteRequest {
            target: &target,
            app: AppKind::Claude,
            backup_dir: &backup_dir,
            expected_hash: &current.content_hash,
            expected_target_existed: true,
            rendered: &rendered,
            reason: "claude-gateway-stop",
        },
        |outcome| {
            gateway.reconcile_restored(local, AppKind::Claude, || {
                local
                    .configuration()
                    .record_config_write(ConfigWriteRecord {
                        app: AppKind::Claude,
                        profile_id: None,
                        profile_name: None,
                        content_hash: outcome.final_hash.clone(),
                        backup_id: outcome.backup.id.clone(),
                        at: outcome.backup.created_at.clone(),
                        operation: WriteOperation::Restore,
                    })
                    .map_err(|e| e.to_string())
            })
        },
    );
    transaction::finish(local, gateway, result)
}

#[tauri::command]
pub(crate) async fn preview_claude_gateway_stop(
    app: AppHandle,
) -> Result<ClaudeGatewayStopPreview, CommandError> {
    let local = state(&app)?;
    let gateway = app.state::<GatewayController>().inner().clone();
    blocking(move || candidate(&local, &gateway, None).map(|(preview, _)| preview)).await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeGatewayStopOutcome {
    backup: BackupRecord,
    final_hash: String,
    warnings: Vec<String>,
}

#[tauri::command]
pub(crate) async fn stop_claude_gateway(
    app: AppHandle,
    preview: ClaudeGatewayStopPreview,
    confirm_write: bool,
) -> Result<ClaudeGatewayStopOutcome, CommandError> {
    require_write_confirmation(confirm_write, "停用 Claude 网关并恢复接管前连接")?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    let local = state(&app)?;
    let gateway = app.state::<GatewayController>().inner().clone();
    observe(RuntimeLogAction::BackupRestored, async move {
        blocking(move || {
            let _guard = gate.lock().map_err(failure)?;
            ensure_profile_save_recovered(&app)?;
            stop(&local, &gateway, &preview).map(|outcome| ClaudeGatewayStopOutcome {
                backup: outcome.backup,
                final_hash: outcome.final_hash,
                warnings: outcome.warnings,
            })
        })
        .await
    })
    .await
}

pub(crate) fn restore_on_exit(app: &AppHandle) -> Result<(), String> {
    let gateway = app.state::<GatewayController>();
    if !gateway.has_active_route_for(AppKind::Claude) {
        return Ok(());
    }
    let local = LocalState::from_app(app)?;
    if !crate::gateway::failover::load(local.root())?
        .traffic
        .restore_on_exit
    {
        return Ok(());
    }
    let lock = app.state::<ConfigWriteGate>().shared();
    let _guard = lock
        .try_lock()
        .map_err(|_| "配置写入正在进行，已取消退出；请稍后再试".to_string())?;
    ensure_profile_save_recovered(app).map_err(|e| e.message)?;
    let (preview, _) = candidate(&local, &gateway, None).map_err(|e| e.message)?;
    stop(&local, &gateway, &preview)
        .map(|_| ())
        .map_err(|e| e.message)
}

