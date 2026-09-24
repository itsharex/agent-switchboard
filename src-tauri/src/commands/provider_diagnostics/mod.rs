//! Read-only provider diagnosis and narrowly scoped, previewed repairs.
mod checks;
mod repairs;
mod snapshot;
mod tickets;

pub use repairs::*;
pub(crate) use tickets::ProviderRepairPreparations;

use super::error::{blocking, operation_error, state, store_error, CommandError};
use asb_core::contracts::{AppKind, LocalizedMessage, ProviderProfile, RouteMode};
use serde::Serialize;
use tauri::{AppHandle, Manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDiagnosticCheck {
    category: &'static str,
    status: &'static str,
    message: LocalizedMessage,
    suggestion: Option<LocalizedMessage>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDiagnosticsReport {
    profile_id: String,
    app: AppKind,
    profile_name: String,
    official: bool,
    active: bool,
    endpoint: Option<String>,
    checks: Vec<ProviderDiagnosticCheck>,
    can_repair: bool,
}

struct SavedProvider {
    profile: ProviderProfile,
    revision: String,
}

fn saved_provider(state: &crate::local_state::LocalState, id: &str) -> Result<SavedProvider, CommandError> {
    let store = state.configuration();
    if let Some(record) = store.list_codex_providers().map_err(store_error)?
        .into_iter().find(|record| record.profile.id == id)
    {
        let (file, revision) = store.find_codex_provider_with_revision(&record.profile.id).map_err(store_error)?;
        return Ok(SavedProvider { profile: file.client_projection().into_profile(AppKind::Codex), revision });
    }
    let record = store.find_provider_record(id)
        .map_err(|error| operation_error("profile-not-found", error))?;
    Ok(SavedProvider { profile: record.profile, revision: record.file_hash })
}

fn message(key: &str, params: serde_json::Value, text: &str) -> LocalizedMessage {
    LocalizedMessage { key: format!("providerDiagnostics.backend.{key}"), params: Some(params), text: text.into() }
}

fn check(category: &'static str, status: &'static str, key: &str, text: &str, suggestion: Option<(&str, &str)>) -> ProviderDiagnosticCheck {
    ProviderDiagnosticCheck {
        category, status, message: message(key, serde_json::json!({}), text),
        suggestion: suggestion.map(|(key, text)| message(key, serde_json::json!({}), text)),
    }
}

fn failed(key: &'static str, text: &str) -> CommandError {
    CommandError::keyed("provider-repair-unavailable", key, text)
}

#[tauri::command]
pub async fn diagnose_provider(app: AppHandle, profile_id: String) -> Result<ProviderDiagnosticsReport, CommandError> {
    let local = state(&app)?;
    let gateway = app.state::<crate::gateway::GatewayController>().inner().clone();
    blocking(move || diagnose(&local, &gateway, &profile_id)).await
}

fn diagnose(local: &crate::local_state::LocalState, gateway: &crate::gateway::GatewayController, id: &str) -> Result<ProviderDiagnosticsReport, CommandError> {
    let saved = saved_provider(local, id)?;
    let profile = &saved.profile;
    let status = super::config_status_report(local, gateway)?.into_iter()
        .find(|status| status.app == profile.app)
        .ok_or_else(|| failed("providerDiagnostics.backend.statusMissing", "无法读取客户端配置状态"))?;
    let active = status.active_profile_id.as_deref() == Some(id);
    let mut checks = checks::profile_checks(profile);
    checks.push(checks::environment(profile.app));
    let projection = super::switching::plan::build_plan(local, gateway, id);
    let preview = match &projection {
        Ok(projection) => repairs::repair_file(local, gateway, projection),
        Err(error) => Err(error.clone()),
    };
    checks.push(checks::configuration(&status, active, preview.as_ref()));
    checks.push(checks::route(profile, active, gateway.observe(local), projection.as_ref()));
    let can_repair = active && status.syntax_ok && status.read_error.is_none()
        && status.recovery_issue.is_none() && status.client_settings_error.is_none()
        && preview.as_ref().is_ok_and(|file| repairs::differs(file));
    Ok(ProviderDiagnosticsReport {
        profile_id: id.into(), app: profile.app, profile_name: profile.name.clone(),
        official: profile.route_mode == RouteMode::Official, active,
        endpoint: checks::safe_endpoint(profile), checks, can_repair,
    })
}
