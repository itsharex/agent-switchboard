//! Codex gateway policy changes use the existing preview/executor boundary.
pub(super) mod journal;
mod preparations;
pub(crate) use preparations::CodexPolicyPreparations;

use crate::commands::error::{blocking, observe, require_write_confirmation, state, CommandError};
use crate::commands::ConfigWriteGate;
use crate::gateway::{
    codex::policy::{self, CodexGatewayPolicy},
    GatewayController,
};
use crate::local_state::LocalState;
use serde::Serialize;
use tauri::{AppHandle, Manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexPolicyView {
    policy: CodexGatewayPolicy,
    revision: String,
    active_profile_id: Option<String>,
    providers: Vec<CodexPolicyProvider>,
    pending_recovery: bool,
    warning: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexPolicyProvider {
    id: String,
    name: String,
    file_hash: String,
    in_queue: bool,
    health: Vec<crate::gateway::codex::CodexEndpointHealth>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexPolicyPreparation {
    preparation_id: String,
    profile_id: String,
    preview: asb_switch::FilePreview,
}

fn view(state: &LocalState, gateway: &GatewayController) -> Result<CodexPolicyView, CommandError> {
    let (policy, revision) = policy::load(state.root()).map_err(policy_error)?;
    let providers = state
        .configuration()
        .codex_provider_snapshots()
        .map_err(crate::commands::error::store_error)?
        .into_iter()
        .map(|(file, file_hash)| {
            let health = gateway
                .codex_endpoint_health(&file, &policy)
                .map_err(policy_error)?;
            Ok(CodexPolicyProvider {
                in_queue: policy.provider_ids.contains(&file.profile.id),
                id: file.profile.id,
                name: file.profile.name,
                file_hash,
                health,
            })
        })
        .collect::<Result<Vec<_>, CommandError>>()?;
    let active_profile_id = state
        .target(asb_core::AppKind::Codex)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| {
            gateway
                .active_profile_id(asb_core::AppKind::Codex, &text)
                .ok()
                .flatten()
        });
    Ok(CodexPolicyView {
        policy,
        revision,
        providers,
        active_profile_id,
        pending_recovery: policy::pending_path(state.root()).exists(),
        warning: gateway.codex_health_warning(),
    })
}
fn policy_error(message: impl Into<String>) -> CommandError {
    CommandError::new("codex-policy-invalid", message)
}
fn recovery_error(message: impl Into<String>) -> CommandError {
    CommandError::new("codex-policy-recovery-required", message)
}

#[tauri::command]
pub(crate) async fn get_codex_gateway_policy(
    app: AppHandle,
) -> Result<CodexPolicyView, CommandError> {
    let state = state(&app)?;
    let gateway = app.state::<GatewayController>().inner().clone();
    blocking(move || view(&state, &gateway)).await
}
#[tauri::command]
pub(crate) async fn prepare_codex_gateway_policy(
    app: AppHandle,
    profile_id: String,
    policy: CodexGatewayPolicy,
) -> Result<CodexPolicyPreparation, CommandError> {
    let pending = app.state::<CodexPolicyPreparations>().inner().clone();
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    let gateway = app.state::<GatewayController>().inner().clone();
    let state = state(&app)?;
    blocking(move || {
        let _guard = gate.lock().map_err(policy_error)?;
        super::ensure_profile_save_recovered(&app)?;
        let prepared = preparations::prepare(&state, &gateway, profile_id.clone(), policy)?;
        let preview = prepared.preview.clone();
        Ok(CodexPolicyPreparation {
            preparation_id: pending.insert(prepared)?,
            profile_id,
            preview,
        })
    })
    .await
}
#[tauri::command]
pub(crate) async fn cancel_codex_gateway_policy(
    app: AppHandle,
    preparation_id: String,
) -> Result<(), CommandError> {
    app.state::<CodexPolicyPreparations>()
        .cancel(&preparation_id)
}
#[tauri::command]
pub(crate) async fn commit_codex_gateway_policy(
    app: AppHandle,
    preparation_id: String,
    confirm_write: bool,
) -> Result<asb_switch::SwitchOutcome, CommandError> {
    require_write_confirmation(confirm_write, "切换 Codex 网关接管和故障转移策略")?;
    let prepared = app
        .state::<CodexPolicyPreparations>()
        .take(&preparation_id)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    let gateway = app.state::<GatewayController>().inner().clone();
    let state = state(&app)?;
    let refresh = app.clone();
    let result = observe(
        crate::runtime_log::RuntimeLogAction::ProfileUpdated,
        async move {
            blocking(move || {
                let _guard = gate.lock().map_err(policy_error)?;
                super::ensure_profile_save_recovered(&app)?;
                preparations::validate(&state, &gateway, &prepared)?;
                commit_prepared(&state, &gateway, &prepared)
            })
            .await
        },
    )
    .await;
    crate::tray::refresh(&refresh);
    result
}
fn commit_prepared(
    state: &LocalState,
    gateway: &GatewayController,
    prepared: &preparations::Prepared,
) -> Result<asb_switch::SwitchOutcome, CommandError> {
    journal::begin(state, gateway, prepared).map_err(recovery_error)?;
    let outcome = policy::save(state.root(), &prepared.policy)
        .map_err(policy_error)
        .and_then(|_| {
            let preview = &prepared.preview;
            super::plan::execute_projection_with_auth(
                state,
                gateway,
                &prepared.projection,
                &preview.content_hash,
                &preview.rendered_hash,
                preview.auth_hash.as_deref(),
                preview.auth_existed,
                preview.auth_rendered_hash.as_deref(),
            )
        });
    match outcome {
        Ok(outcome) => {
            journal::clear(state.root()).map_err(recovery_error)?;
            Ok(outcome)
        }
        Err(error) => {
            super::transaction::recover(state, gateway).map_err(|recovery| {
                recovery_error(format!("{}；客户端事务恢复失败：{recovery}", error.message))
            })?;
            journal::recover(state, gateway, true).map_err(|recovery| {
                recovery_error(format!("{}；策略恢复失败：{recovery}", error.message))
            })?;
            Err(error)
        }
    }
}

pub(crate) fn recover_on_startup(
    state: &LocalState,
    gateway: &GatewayController,
) -> Result<(), String> {
    journal::recover(state, gateway, false)
}
#[tauri::command]
pub(crate) async fn recover_codex_gateway_policy(
    app: AppHandle,
) -> Result<CodexPolicyView, CommandError> {
    let state = state(&app)?;
    let gateway = app.state::<GatewayController>().inner().clone();
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(policy_error)?;
        super::transaction::recover(&state, &gateway).map_err(recovery_error)?;
        journal::recover(&state, &gateway, false).map_err(recovery_error)?;
        view(&state, &gateway)
    })
    .await
}
#[tauri::command]
pub(crate) async fn discard_codex_gateway_policy(
    app: AppHandle,
    expected_config_hash: String,
    confirm_write: bool,
) -> Result<CodexPolicyView, CommandError> {
    require_write_confirmation(
        confirm_write,
        "备份并放弃 Codex 网关事务（保留当前客户端文件）",
    )?;
    let state = state(&app)?;
    let gateway = app.state::<GatewayController>().inner().clone();
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(policy_error)?;
        journal::abandon(&state, &gateway, &expected_config_hash).map_err(recovery_error)?;
        view(&state, &gateway)
    })
    .await
}
#[tauri::command]
pub(crate) async fn reset_codex_provider_health(
    app: AppHandle,
    profile_id: String,
    confirm_write: bool,
) -> Result<CodexPolicyView, CommandError> {
    require_write_confirmation(confirm_write, "重置 Codex 供应商熔断状态")?;
    let state = state(&app)?;
    let gateway = app.state::<GatewayController>().inner().clone();
    blocking(move || {
        gateway
            .reset_codex_provider_health(&profile_id)
            .map_err(policy_error)?;
        view(&state, &gateway)
    })
    .await
}

/// Read-only the source application scan. The proposal it returns is applied only through
/// `prepare_codex_gateway_policy` / `commit_codex_gateway_policy`, so the
/// source import shares the one preview/confirm transaction and never gains
/// a second write path.
#[tauri::command]
pub(crate) async fn scan_codex_failover_source(
    app: AppHandle,
    source_path: String,
) -> Result<crate::ccswitch_source::codex_failover::CodexFailoverSourceScan, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        crate::ccswitch_source::codex_failover::scan(std::path::Path::new(&source_path), &state)
            .map_err(|message| CommandError::new("codex-failover-scan-failed", message))
    })
    .await
}

#[cfg(test)]
mod tests;
