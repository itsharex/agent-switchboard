//! Gateway commands: the status observation, listener retry, and the
//! prepare/commit port-change transaction. Read-only observation never
//! blocks; the transaction refuses any write without an explicit confirm.

use super::error::{blocking, require_write_confirmation, state, CommandError};
use crate::commands::ConfigWriteGate;
use crate::gateway::port_change::{self, PortChangePreparations};
use crate::gateway::{GatewayObservation, GatewayPortChangePlan, GatewayPortChangeResult};
use tauri::{AppHandle, Manager};

fn gateway(app: &AppHandle) -> Result<crate::gateway::GatewayController, CommandError> {
    Ok(app
        .state::<crate::gateway::GatewayController>()
        .inner()
        .clone())
}

fn local(app: &AppHandle) -> Result<crate::local_state::LocalState, CommandError> {
    state(app)
}

fn preparations(app: &AppHandle) -> Result<PortChangePreparations, CommandError> {
    app.try_state::<PortChangePreparations>()
        .map(|state| state.inner().clone())
        .ok_or_else(|| CommandError::new("gateway-unavailable", "端口修改准备状态尚未初始化"))
}

fn write_gate(app: &AppHandle) -> Result<ConfigWriteGate, CommandError> {
    app.try_state::<ConfigWriteGate>()
        .map(|gate| gate.inner().clone())
        .ok_or_else(|| CommandError::new("gateway-unavailable", "写入闸门尚未初始化"))
}

/// Read-only gateway observation for the status page.
#[tauri::command]
pub async fn gateway_status(app: AppHandle) -> Result<GatewayObservation, CommandError> {
    let gateway = gateway(&app)?;
    let local = local(&app)?;
    blocking(move || Ok(gateway.observe(&local))).await
}

/// Retries binding the configured port after a failed start. Succeeding
/// publishes the restored routes; failing keeps the structured report.
#[tauri::command]
pub async fn gateway_retry_bind(app: AppHandle) -> Result<GatewayObservation, CommandError> {
    let gateway = gateway(&app)?;
    let local = local(&app)?;
    let gate = write_gate(&app)?;
    blocking(move || {
        let _write_guard = gate
            .lock()
            .map_err(|error| CommandError::new("gateway-retry-gate-unavailable", error))?;
        crate::commands::switching::codex_policy::recover_on_startup(&local, &gateway)
            .map_err(|error| CommandError::new("codex-policy-recovery-required", error))?;
        Ok(gateway.retry_bind(&local))
    })
    .await
}

/// Validates the new port, holds its socket, and identifies the client
/// configurations that will be rewritten. No file is modified.
#[tauri::command]
pub async fn gateway_prepare_port_change(
    app: AppHandle,
    new_port: u16,
) -> Result<GatewayPortChangePlan, CommandError> {
    let gateway = gateway(&app)?;
    let local = local(&app)?;
    let preparations = preparations(&app)?;
    blocking(move || {
        port_change::prepare(&gateway, &local, &preparations, new_port)
            .map_err(|error| CommandError::new("gateway-port-change-invalid", error))
    })
    .await
}

/// Applies the confirmed port change as one recoverable transaction.
#[tauri::command]
pub async fn gateway_commit_port_change(
    app: AppHandle,
    preparation_id: String,
    confirm_write: bool,
) -> Result<GatewayPortChangeResult, CommandError> {
    require_write_confirmation(confirm_write, "修改网关端口")?;
    let gateway = gateway(&app)?;
    let local = local(&app)?;
    let preparations = preparations(&app)?;
    let gate = write_gate(&app)?;
    let recovery_app = app.clone();
    let refresh_app = app.clone();
    let result = blocking(move || {
        let _write_guard = gate
            .lock()
            .map_err(|error| CommandError::new("gateway-port-change-gate-unavailable", error))?;
        crate::commands::switching::ensure_profile_save_recovered(&recovery_app)?;
        port_change::commit(&gateway, &local, &preparations, &preparation_id)
            .map_err(|error| CommandError::new("gateway-port-change-failed", error))
    })
    .await;
    if result.is_ok() {
        crate::tray::refresh(&refresh_app);
    }
    result
}

/// Releases a prepared socket when the user closes or cancels the preview.
/// It is idempotent because a completed or expired preview already released
/// the same process-local resource.
#[tauri::command]
pub async fn gateway_cancel_port_change(
    app: AppHandle,
    preparation_id: String,
) -> Result<(), CommandError> {
    let preparations = preparations(&app)?;
    blocking(move || {
        preparations
            .cancel(&preparation_id)
            .map(|_| ())
            .map_err(|error| CommandError::new("gateway-port-change-cancel-failed", error))
    })
    .await
}

/// Discards a blocked port-change recovery after the user explicitly chose
/// to keep the current externally modified configuration.
#[tauri::command]
pub async fn gateway_discard_port_change(
    app: AppHandle,
    confirm_write: bool,
) -> Result<GatewayObservation, CommandError> {
    require_write_confirmation(confirm_write, "放弃端口修改恢复")?;
    let gateway = gateway(&app)?;
    let local = local(&app)?;
    let gate = write_gate(&app)?;
    blocking(move || {
        let _write_guard = gate
            .lock()
            .map_err(|error| CommandError::new("gateway-port-change-gate-unavailable", error))?;
        port_change::discard_blocked(&gateway, &local)
            .map_err(|error| CommandError::new("gateway-recovery-discard-failed", error))?;
        Ok(gateway.observe(&local))
    })
    .await
}
