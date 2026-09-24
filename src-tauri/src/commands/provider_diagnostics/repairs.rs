use super::{failed, saved_provider, snapshot, tickets::{CompletedRepair, ProviderRepairPreparations, Ticket}};
use crate::{commands::{error::{blocking, observe, require_write_confirmation, state, CommandError}, switching, ConfigWriteGate}, runtime_log::RuntimeLogAction};
use asb_core::{AppKind, KeyChange};
use asb_switch::{FilePreview, RestoreOutcome, SwitchOutcome};
use serde::Serialize;
use tauri::{AppHandle, Manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRepairPreview { preparation_id: String, file: FilePreview }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRepairResult { repair_id: String, can_undo: bool, outcome: SwitchOutcome }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRepairUndoPreview { preparation_id: String, changes: Vec<KeyChange> }

fn tickets(app: &AppHandle) -> Result<ProviderRepairPreparations, CommandError> {
    app.try_state::<ProviderRepairPreparations>().map(|state| state.inner().clone())
        .ok_or_else(super::tickets::unavailable)
}

fn gate(app: &AppHandle) -> Result<ConfigWriteGate, CommandError> {
    app.try_state::<ConfigWriteGate>().map(|state| state.inner().clone())
        .ok_or_else(super::tickets::unavailable)
}

fn ensure_active(local: &crate::local_state::LocalState, gateway: &crate::gateway::GatewayController, app: AppKind, id: &str) -> Result<(), CommandError> {
    let status = crate::commands::config_status_report(local, gateway)?.into_iter()
        .find(|status| status.app == app).ok_or_else(snapshot::stale)?;
    if status.active_profile_id.as_deref() != Some(id) || !status.syntax_ok || status.read_error.is_some()
        || status.recovery_issue.is_some() || status.client_settings_error.is_some()
    {
        return Err(failed("providerDiagnostics.backend.activeRequired", "只能修复当前可识别的活动供应商；请先处理客户端配置错误或显式切换供应商"));
    }
    Ok(())
}

pub(super) fn repair_file(local: &crate::local_state::LocalState, gateway: &crate::gateway::GatewayController, projection: &crate::gateway::GatewayProjection) -> Result<FilePreview, CommandError> {
    let mut file = switching::plan::preview_projection(local, projection)?;
    if file.auth_hash != file.auth_rendered_hash || file.auth_existed != file.auth_rendered_existed {
        file.preview.changes.push(KeyChange {
            key: "auth.json".into(),
            kind: if file.auth_rendered_existed == Some(true) { asb_core::ChangeKind::Set } else { asb_core::ChangeKind::Remove },
            before: file.auth_existed.filter(|value| *value).map(|_| "[redacted]".into()),
            after: file.auth_rendered_existed.filter(|value| *value).map(|_| "[redacted]".into()),
        });
    }
    if let Some(change) = snapshot::catalog_change(local, projection)? {
        if change.after.is_some() {
            file.preview.warnings.push(super::message("catalogRetained", serde_json::json!({}),
                "本次会生成缺失的不可变模型目录；撤回只恢复配置与认证，不删除生成的模型目录。"));
        }
        file.preview.changes.push(change);
    }
    let target = local.target(projection.plan.app()).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let current = snapshot::optional_file(&target)?;
    switching::validate_client_configuration_backup(local, gateway, projection.plan.app(), current.as_deref().unwrap_or(""), current.is_some())?;
    Ok(file)
}

pub(super) fn differs(file: &FilePreview) -> bool {
    !file.preview.changes.is_empty()
}

#[tauri::command]
pub async fn prepare_provider_repair(app: AppHandle, profile_id: String) -> Result<ProviderRepairPreview, CommandError> {
    let local = state(&app)?;
    let gateway = app.state::<crate::gateway::GatewayController>().inner().clone();
    let tickets = tickets(&app)?;
    blocking(move || {
        let kind = saved_provider(&local, &profile_id)?.profile.app;
        ensure_active(&local, &gateway, kind, &profile_id)?;
        let projection = switching::plan::build_plan(&local, &gateway, &profile_id)?;
        let before = snapshot::capture(&local, kind, &profile_id, &projection)?;
        let file = repair_file(&local, &gateway, &projection)?;
        if !differs(&file) { return Err(failed("providerDiagnostics.backend.noChanges", "当前供应商配置已一致，无需修复")); }
        snapshot::unchanged(&local, kind, &profile_id, &before)?;
        let preparation_id = tickets.issue(Ticket::Repair { app: kind, profile_id, before, file: file.clone() })?;
        Ok(ProviderRepairPreview { preparation_id, file })
    }).await
}

#[tauri::command]
pub async fn commit_provider_repair(app: AppHandle, preparation_id: String, confirm_write: bool) -> Result<ProviderRepairResult, CommandError> {
    let tickets = tickets(&app)?;
    let Ticket::Repair { app: kind, profile_id, before, file } = tickets.take(&preparation_id)? else { return Err(super::tickets::unavailable()); };
    require_write_confirmation(confirm_write, "修复当前供应商配置")?;
    let local = state(&app)?;
    let gateway = app.state::<crate::gateway::GatewayController>().inner().clone();
    let gate = gate(&app)?;
    observe(RuntimeLogAction::ConfigurationSwitched, blocking(move || {
        let _guard = gate.lock().map_err(|error| CommandError::new("config-write-gate-unavailable", error))?;
        switching::ensure_profile_save_recovered(&app)?;
        ensure_active(&local, &gateway, kind, &profile_id)?;
        snapshot::unchanged(&local, kind, &profile_id, &before)?;
        let projection = switching::plan::build_plan(&local, &gateway, &profile_id)?;
        let latest = repair_file(&local, &gateway, &projection)?;
        if !same_preview(&latest, &file) { return Err(snapshot::stale()); }
        let outcome = switching::plan::execute_projection_with_auth(&local, &gateway, &projection,
            &file.content_hash, &file.rendered_hash, file.auth_hash.as_deref(), file.auth_existed, file.auth_rendered_hash.as_deref())?;
        crate::tray::refresh(&app);
        let after = snapshot::verified_after(&local, &profile_id, &before, &projection, &file, &outcome.final_hash)
            .map_err(|_| post_commit_error(&outcome.backup.id))?;
        let can_undo = file.preview.changes.iter().any(|change| !change.key.starts_with("agent-switchboard-codex-"));
        let repair_id = tickets.issue(Ticket::Completed(CompletedRepair {
            app: kind, profile_id, backup_id: outcome.backup.id.clone(), after,
        })).map_err(|_| post_commit_error(&outcome.backup.id))?;
        Ok(ProviderRepairResult { repair_id, can_undo, outcome })
    })).await
}

fn post_commit_error(backup_id: &str) -> CommandError {
    CommandError::localized("provider-repair-committed", "providerDiagnostics.backend.committedWithoutUndo",
        "修复已完成，但即时撤回快照不可用；请使用备份页恢复", serde_json::json!({ "backupId": backup_id }))
}

fn same_preview(left: &FilePreview, right: &FilePreview) -> bool {
    left.content_hash == right.content_hash && left.rendered_hash == right.rendered_hash
        && left.auth_hash == right.auth_hash && left.auth_existed == right.auth_existed
        && left.auth_rendered_hash == right.auth_rendered_hash && left.auth_rendered_existed == right.auth_rendered_existed
        && left.preview.changes == right.preview.changes
}

#[tauri::command]
pub async fn prepare_provider_repair_undo(app: AppHandle, repair_id: String) -> Result<ProviderRepairUndoPreview, CommandError> {
    let local = state(&app)?;
    let gateway = app.state::<crate::gateway::GatewayController>().inner().clone();
    let tickets = tickets(&app)?;
    let completed = tickets.completed(&repair_id)?;
    blocking(move || {
        snapshot::unchanged(&local, completed.app, &completed.profile_id, &completed.after)?;
        let backup = switching::backups::find_backup(&local, &completed.backup_id)?;
        let (fingerprint, changes) = snapshot::restore_preview(&local, &gateway, &backup)?;
        if changes.is_empty() { return Err(failed("providerDiagnostics.backend.noUndoChanges", "没有可撤回的配置或认证差异；生成的不可变模型目录会保留")); }
        snapshot::unchanged(&local, completed.app, &completed.profile_id, &completed.after)?;
        let preparation_id = tickets.issue(Ticket::Undo { completed, fingerprint })?;
        Ok(ProviderRepairUndoPreview { preparation_id, changes })
    }).await
}

#[tauri::command]
pub async fn commit_provider_repair_undo(app: AppHandle, preparation_id: String, confirm_write: bool) -> Result<RestoreOutcome, CommandError> {
    let Ticket::Undo { completed, fingerprint } = tickets(&app)?.take(&preparation_id)? else { return Err(super::tickets::unavailable()); };
    require_write_confirmation(confirm_write, "撤回本次供应商修复")?;
    let local = state(&app)?;
    let gateway = app.state::<crate::gateway::GatewayController>().inner().clone();
    let gate = gate(&app)?;
    observe(RuntimeLogAction::SwitchUndone, blocking(move || {
        let _guard = gate.lock().map_err(|error| CommandError::new("config-write-gate-unavailable", error))?;
        switching::ensure_profile_save_recovered(&app)?;
        snapshot::unchanged(&local, completed.app, &completed.profile_id, &completed.after)?;
        let backup = switching::backups::find_backup(&local, &completed.backup_id)?;
        if snapshot::restore_preview(&local, &gateway, &backup)?.0 != fingerprint { return Err(snapshot::stale()); }
        let expected = snapshot::restore_expectation(&local, completed.app, &completed.after)?;
        let outcome = switching::backups::run_restore(&local, &gateway, &backup, Some(&expected))?;
        crate::tray::refresh(&app);
        Ok(outcome)
    })).await
}

#[tauri::command]
pub async fn cancel_provider_repair_preparation(app: AppHandle, preparation_id: String) -> Result<(), CommandError> {
    tickets(&app)?.cancel(&preparation_id)
}
