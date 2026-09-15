//! Claude failover policy commands.
//!
//! The policy is application-owned state. Provider credentials remain in the
//! existing configuration store and are never returned by this surface.

use super::error::{
    blocking, observe, require_write_confirmation, state, store_error, CommandError,
};
use crate::commands::ConfigWriteGate;
use crate::gateway::failover::{self, ClaudeFailoverPolicy};
use crate::gateway::GatewayController;
use crate::runtime_log::RuntimeLogAction;
use asb_core::contracts::{AppKind, RouteMode};
use serde::Serialize;
use std::collections::HashSet;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeFailoverProvider {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) file_hash: String,
    pub(crate) in_queue: bool,
    pub(crate) routeable: bool,
    pub(crate) health: crate::gateway::ProviderHealthSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeFailoverView {
    pub(crate) policy: ClaudeFailoverPolicy,
    pub(crate) providers: Vec<ClaudeFailoverProvider>,
    pub(crate) warnings: Vec<String>,
}

fn gateway(app: &AppHandle) -> Result<GatewayController, CommandError> {
    Ok(app.state::<GatewayController>().inner().clone())
}

fn write_gate(app: &AppHandle) -> Result<ConfigWriteGate, CommandError> {
    app.try_state::<ConfigWriteGate>()
        .map(|gate| gate.inner().clone())
        .ok_or_else(|| CommandError::new("claude-failover-gate-unavailable", "写入闸门尚未初始化"))
}

pub(super) fn read_policy(
    state: &crate::local_state::LocalState,
) -> Result<ClaudeFailoverPolicy, CommandError> {
    failover::load(state.root())
        .map_err(|error| CommandError::new("claude-failover-unavailable", error))
}

fn claude_providers(
    state: &crate::local_state::LocalState,
) -> Result<Vec<asb_core::contracts::ProviderRecord>, CommandError> {
    state
        .configuration()
        .list_providers()
        .map(|records| {
            records
                .into_iter()
                .filter(|record| {
                    record.profile.app == AppKind::Claude
                        && record.profile.route_mode == RouteMode::Custom
                })
                .collect()
        })
        .map_err(store_error)
}

pub(super) fn validate_policy(
    state: &crate::local_state::LocalState,
    policy: &ClaudeFailoverPolicy,
) -> Result<ClaudeFailoverPolicy, CommandError> {
    policy
        .traffic
        .validate()
        .map_err(|error| CommandError::new("claude-traffic-policy-invalid", error))?;
    let providers = claude_providers(state)?;
    let available = providers
        .iter()
        .filter(|record| record.profile.connection.claude_native.is_none())
        .map(|record| record.profile.id.as_str())
        .collect::<HashSet<_>>();
    let mut normalized = policy.clone().normalized();
    if let Some(id) = normalized
        .provider_ids
        .iter()
        .find(|id| !available.contains(id.as_str()))
    {
        return Err(CommandError::new(
            "claude-failover-provider-invalid",
            format!("Claude 故障转移队列包含不存在或非 Claude 供应商：{id}"),
        ));
    }
    if normalized.enabled && normalized.provider_ids.is_empty() && available.is_empty() {
        return Err(CommandError::new(
            "claude-failover-provider-invalid",
            "没有可用于 Claude 故障转移的供应商",
        ));
    }
    normalized.normalize();
    Ok(normalized)
}

pub(super) fn view(
    state: &crate::local_state::LocalState,
    gateway: &GatewayController,
) -> Result<ClaudeFailoverView, CommandError> {
    let policy = read_policy(state)?;
    let queued = policy
        .provider_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let providers = claude_providers(state)?
        .into_iter()
        .map(|record| {
            let routeable = record.profile.connection.claude_native.is_none();
            let health = if routeable {
                gateway
                    .claude_health_snapshot(&record.profile)
                    .map_err(|error| CommandError::new("claude-health-unavailable", error))?
            } else {
                Default::default()
            };
            let id = record.profile.id;
            Ok(ClaudeFailoverProvider {
                in_queue: queued.contains(id.as_str()),
                routeable,
                id,
                name: record.profile.name,
                file_hash: record.file_hash,
                health,
            })
        })
        .collect::<Result<Vec<_>, CommandError>>()?;
    let mut warnings: Vec<String> = gateway.claude_health_warning().into_iter().collect();
    if (policy.enabled || policy.takeover) && !gateway.has_active_route_for(AppKind::Claude) {
        warnings
            .push("Claude 策略已保存，但当前尚未接管；请在供应商页预览并重新应用档案后生效".into());
    }
    Ok(ClaudeFailoverView {
        policy,
        providers,
        warnings,
    })
}

pub(super) fn save_and_refresh(
    state: &crate::local_state::LocalState,
    gateway: &GatewayController,
    previous: &ClaudeFailoverPolicy,
    policy: &ClaudeFailoverPolicy,
) -> Result<(), CommandError> {
    failover::save(state.root(), policy)
        .map_err(|error| CommandError::new("claude-failover-save-failed", error))?;
    if let Err(error) = gateway.refresh_claude_candidates(state) {
        if let Err(rollback) = failover::save(state.root(), previous) {
            return Err(CommandError::new("claude-failover-recovery-required",
                format!("Claude 路由刷新失败：{error}；策略回滚也失败：{rollback}。请修复策略文件并重新应用供应商")));
        }
        return Err(CommandError::new("claude-failover-refresh-failed", error));
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_claude_failover(
    app: AppHandle,
) -> Result<ClaudeFailoverView, CommandError> {
    let state = state(&app)?;
    let gateway = gateway(&app)?;
    blocking(move || view(&state, &gateway)).await
}

#[tauri::command]
pub(crate) async fn set_claude_failover_policy(
    app: AppHandle,
    policy: ClaudeFailoverPolicy,
    confirm_write: bool,
) -> Result<ClaudeFailoverView, CommandError> {
    require_write_confirmation(confirm_write, "修改 Claude 故障转移策略")?;
    let gate = write_gate(&app)?;
    let result = observe(RuntimeLogAction::ProfileUpdated, async move {
        let state = state(&app)?;
        let gateway = gateway(&app)?;
        blocking(move || {
            let _write_guard = gate
                .lock()
                .map_err(|error| CommandError::new("claude-failover-gate-unavailable", error))?;
            let previous = read_policy(&state)?;
            let next = validate_policy(&state, &policy)?;
            save_and_refresh(&state, &gateway, &previous, &next)?;
            view(&state, &gateway)
        })
        .await
    })
    .await?;
    Ok(result)
}

#[tauri::command]
pub(crate) async fn set_claude_failover_enabled(
    app: AppHandle,
    enabled: bool,
    confirm_write: bool,
) -> Result<ClaudeFailoverView, CommandError> {
    require_write_confirmation(confirm_write, "切换 Claude 故障转移")?;
    let gate = write_gate(&app)?;
    let result = observe(RuntimeLogAction::ProfileUpdated, async move {
        let state = state(&app)?;
        let gateway = gateway(&app)?;
        blocking(move || {
            let _write_guard = gate
                .lock()
                .map_err(|error| CommandError::new("claude-failover-gate-unavailable", error))?;
            let previous = read_policy(&state)?;
            let mut next = previous.clone();
            next.enabled = enabled;
            if enabled && next.provider_ids.is_empty() {
                if let Some(route_id) = gateway.active_profile_id_for(AppKind::Claude) {
                    next.provider_ids.push(route_id);
                } else if let Some(provider) = claude_providers(&state)?.first() {
                    next.provider_ids.push(provider.profile.id.clone());
                }
            }
            let next = validate_policy(&state, &next)?;
            save_and_refresh(&state, &gateway, &previous, &next)?;
            view(&state, &gateway)
        })
        .await
    })
    .await?;
    Ok(result)
}

#[tauri::command]
pub(crate) async fn add_claude_failover_provider(
    app: AppHandle,
    provider_id: String,
    confirm_write: bool,
) -> Result<ClaudeFailoverView, CommandError> {
    require_write_confirmation(confirm_write, "添加 Claude 故障转移供应商")?;
    let gate = write_gate(&app)?;
    let result = observe(RuntimeLogAction::ProfileUpdated, async move {
        let state = state(&app)?;
        let gateway = gateway(&app)?;
        blocking(move || {
            let _write_guard = gate
                .lock()
                .map_err(|error| CommandError::new("claude-failover-gate-unavailable", error))?;
            let previous = read_policy(&state)?;
            let mut next = previous.clone();
            next.provider_ids.push(provider_id);
            let next = validate_policy(&state, &next)?;
            save_and_refresh(&state, &gateway, &previous, &next)?;
            view(&state, &gateway)
        })
        .await
    })
    .await?;
    Ok(result)
}

#[tauri::command]
pub(crate) async fn remove_claude_failover_provider(
    app: AppHandle,
    provider_id: String,
    confirm_write: bool,
) -> Result<ClaudeFailoverView, CommandError> {
    require_write_confirmation(confirm_write, "移除 Claude 故障转移供应商")?;
    let gate = write_gate(&app)?;
    let result = observe(RuntimeLogAction::ProfileUpdated, async move {
        let state = state(&app)?;
        let gateway = gateway(&app)?;
        blocking(move || {
            let _write_guard = gate
                .lock()
                .map_err(|error| CommandError::new("claude-failover-gate-unavailable", error))?;
            let previous = read_policy(&state)?;
            let mut next = previous.clone();
            next.provider_ids.retain(|id| id != &provider_id);
            save_and_refresh(&state, &gateway, &previous, &next.normalized())?;
            view(&state, &gateway)
        })
        .await
    })
    .await?;
    Ok(result)
}

#[tauri::command]
pub(crate) async fn reorder_claude_failover_providers(
    app: AppHandle,
    ordered_ids: Vec<String>,
    confirm_write: bool,
) -> Result<ClaudeFailoverView, CommandError> {
    require_write_confirmation(confirm_write, "排序 Claude 故障转移供应商")?;
    let gate = write_gate(&app)?;
    let result = observe(RuntimeLogAction::ProfileUpdated, async move {
        let state = state(&app)?;
        let gateway = gateway(&app)?;
        blocking(move || {
            let _write_guard = gate
                .lock()
                .map_err(|error| CommandError::new("claude-failover-gate-unavailable", error))?;
            let previous = read_policy(&state)?;
            let existing = failover::normalize_provider_ids(&previous.provider_ids);
            let next_ids = failover::normalize_provider_ids(&ordered_ids);
            if existing.len() != next_ids.len()
                || existing
                    .iter()
                    .any(|id| !next_ids.iter().any(|item| item == id))
            {
                return Err(CommandError::new(
                    "claude-failover-order-invalid",
                    "排序清单必须覆盖当前 Claude 故障转移队列且不得遗漏或重复",
                ));
            }
            let mut next = previous.clone();
            next.provider_ids = next_ids;
            let next = validate_policy(&state, &next)?;
            save_and_refresh(&state, &gateway, &previous, &next)?;
            view(&state, &gateway)
        })
        .await
    })
    .await?;
    Ok(result)
}

pub(crate) fn remove_provider_from_policy(
    state: &crate::local_state::LocalState,
    provider_id: &str,
) -> Result<(), String> {
    if !failover::path(state.root()).exists() {
        return Ok(());
    }
    let mut policy = failover::load(state.root())?;
    policy.provider_ids.retain(|id| id != provider_id);
    failover::save(state.root(), &policy)
}

#[tauri::command]
pub(crate) async fn reset_claude_provider_health(
    app: AppHandle,
    provider_id: String,
    confirm_write: bool,
) -> Result<ClaudeFailoverView, CommandError> {
    require_write_confirmation(confirm_write, "重置 Claude 供应商熔断状态")?;
    let state = state(&app)?;
    let gateway = gateway(&app)?;
    let gate = write_gate(&app)?;
    blocking(move || {
        let _guard = gate
            .lock()
            .map_err(|error| CommandError::new("claude-health-unavailable", error))?;
        let profile = claude_providers(&state)?
            .into_iter()
            .find(|record| record.profile.id == provider_id)
            .ok_or_else(|| {
                CommandError::new(
                    "claude-health-provider-missing",
                    "找不到 Claude 自定义供应商",
                )
            })?
            .profile;
        gateway
            .reset_claude_health(&profile)
            .map_err(|error| CommandError::new("claude-health-reset-failed", error))?;
        view(&state, &gateway)
    })
    .await
}
