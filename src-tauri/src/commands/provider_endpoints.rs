//! CRUD for provider-owned custom upstream endpoints.
//!
//! These entries live in the generic provider file as application metadata.
//! They never change the client-facing Claude configuration directly; a
//! confirmed provider save or the gateway's request-time bookkeeping is the
//! only path that updates them.

use super::error::{
    blocking, observe, operation_error, require_write_confirmation, state, CommandError,
};
use super::switching;
use crate::commands::ConfigWriteGate;
use crate::runtime_log::RuntimeLogAction;
use asb_core::contracts::{AppKind, ProviderEndpoint, ProviderRecord, RouteMode};
use serde::Serialize;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderEndpointsView {
    pub(crate) provider_id: String,
    pub(crate) file_hash: String,
    pub(crate) endpoints: Vec<ProviderEndpoint>,
}

fn write_gate(app: &AppHandle) -> Result<ConfigWriteGate, CommandError> {
    app.try_state::<ConfigWriteGate>()
        .map(|gate| gate.inner().clone())
        .ok_or_else(|| {
            CommandError::keyed(
                "provider-endpoint-unavailable",
                "errors.misc.writeGateNotInitialized",
                "写入闸门尚未初始化",
            )
        })
}

fn view(record: ProviderRecord) -> Result<ProviderEndpointsView, CommandError> {
    if record.profile.route_mode != RouteMode::Custom {
        return Err(CommandError::keyed(
            "provider-endpoint-invalid",
            "errors.misc.officialProviderNoCustomEndpoints",
            "官方登录供应商不支持自定义服务端点",
        ));
    }
    let mut endpoints = record
        .profile
        .connection
        .custom_endpoints
        .into_values()
        .collect::<Vec<_>>();
    endpoints.sort_by(|left, right| {
        right
            .last_used
            .cmp(&left.last_used)
            .then_with(|| right.added_at.cmp(&left.added_at))
            .then_with(|| left.url.cmp(&right.url))
    });
    Ok(ProviderEndpointsView {
        provider_id: record.profile.id,
        file_hash: record.file_hash,
        endpoints,
    })
}

fn reject_active_provider(app: &AppHandle, provider_id: &str) -> Result<(), CommandError> {
    let Some(gateway) = app.try_state::<crate::gateway::GatewayController>() else {
        return Ok(());
    };
    if gateway.active_profile_id_for(AppKind::Claude).as_deref() == Some(provider_id) {
        return Err(CommandError::keyed(
            "provider-endpoint-active",
            "errors.misc.activeProviderEndpointChangeBlocked",
            "当前 Claude 供应商正在使用，请重新应用后再修改服务端点",
        ));
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn list_provider_endpoints(
    app: AppHandle,
    provider_id: String,
) -> Result<ProviderEndpointsView, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let record = state
            .configuration()
            .find_provider_record(&provider_id)
            .map_err(|error| operation_error("provider-endpoint-list-failed", error))?;
        view(record)
    })
    .await
}

#[tauri::command]
pub(crate) async fn add_provider_endpoint(
    app: AppHandle,
    provider_id: String,
    url: String,
    expected_file_hash: String,
    confirm_write: bool,
) -> Result<ProviderEndpointsView, CommandError> {
    require_write_confirmation(confirm_write, "添加供应商服务端点")?;
    let gate = write_gate(&app)?;
    observe(RuntimeLogAction::ProfileUpdated, async move {
        let state = state(&app)?;
        reject_active_provider(&app, &provider_id)?;
        blocking(move || {
            let _write_guard = gate
                .lock()
                .map_err(|error| CommandError::new("provider-endpoint-unavailable", error))?;
            switching::ensure_profile_save_recovered(&app)?;
            let record = state
                .configuration()
                .add_provider_endpoint(&provider_id, &url, &expected_file_hash)
                .map_err(|error| operation_error("provider-endpoint-add-failed", error))?;
            view(record)
        })
        .await
    })
    .await
}

#[tauri::command]
pub(crate) async fn remove_provider_endpoint(
    app: AppHandle,
    provider_id: String,
    url: String,
    expected_file_hash: String,
    confirm_write: bool,
) -> Result<ProviderEndpointsView, CommandError> {
    require_write_confirmation(confirm_write, "删除供应商服务端点")?;
    let gate = write_gate(&app)?;
    observe(RuntimeLogAction::ProfileUpdated, async move {
        let state = state(&app)?;
        reject_active_provider(&app, &provider_id)?;
        blocking(move || {
            let _write_guard = gate
                .lock()
                .map_err(|error| CommandError::new("provider-endpoint-unavailable", error))?;
            switching::ensure_profile_save_recovered(&app)?;
            let record = state
                .configuration()
                .remove_provider_endpoint(&provider_id, &url, &expected_file_hash)
                .map_err(|error| operation_error("provider-endpoint-remove-failed", error))?;
            view(record)
        })
        .await
    })
    .await
}
