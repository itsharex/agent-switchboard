use super::discovery::{codex_import_source, discovery_report};
use super::error::{self, blocking, observe, operation_error, state, store_error, CommandError};
use super::switching;
use crate::local_state::LocalState;
use crate::runtime_log::RuntimeLogAction;
use asb_core::contracts::{AppKind, CodexProviderDraft, CodexProviderRecord, ProviderRecord};
use asb_core::discovery::CodexImportAction;
use std::collections::BTreeMap;
use tauri::Manager;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexImportResult {
    pub id: String,
    pub name: String,
    pub official: bool,
}

#[tauri::command]
pub async fn list_profiles(app: tauri::AppHandle) -> Result<Vec<ProviderRecord>, CommandError> {
    let state = state(&app)?;
    blocking(move || state.configuration().list_providers().map_err(store_error)).await
}

/// Lists only current-format third-party Codex profiles. This dedicated
/// boundary prevents the renderer from treating a generic provider draft as
/// a Codex route definition.
#[tauri::command]
pub async fn list_codex_profiles(
    app: tauri::AppHandle,
) -> Result<Vec<CodexProviderRecord>, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        state
            .configuration()
            .list_codex_providers()
            .map_err(store_error)
    })
    .await
}

#[tauri::command]
pub async fn create_codex_profile(
    app: tauri::AppHandle,
    draft: CodexProviderDraft,
) -> Result<CodexProviderRecord, CommandError> {
    let refresh_app = app.clone();
    let result = observe(RuntimeLogAction::ProfileCreated, async move {
        let state = state(&app)?;
        blocking(move || {
            state
                .configuration()
                .create_codex_provider(draft)
                .map_err(|error| operation_error("codex-profile-create-failed", error))
        })
        .await
    })
    .await;
    if result.is_ok() {
        crate::tray::refresh(&refresh_app);
    }
    result
}

#[tauri::command]
pub async fn delete_codex_profile(
    app: tauri::AppHandle,
    profile_id: String,
    expected_file_hash: String,
) -> Result<(), CommandError> {
    let refresh_app = app.clone();
    let cache_profile_id = profile_id.clone();
    let result = observe(RuntimeLogAction::ProfileDeleted, async move {
        let state = state(&app)?;
        let gateway = app
            .state::<crate::gateway::GatewayController>()
            .inner()
            .clone();
        blocking(move || {
            switching::ensure_profile_save_recovered(&app)?;
            if gateway.uses_profile(&profile_id) {
                return Err(CommandError::new(
                    "gateway-profile-active",
                    "该 Codex 供应商正在被本机协议网关使用；请先切换到官方登录后再删除",
                ));
            }
            super::codex_management::ensure_deletable(&state, &profile_id)?;
            state
                .configuration()
                .delete_codex_provider(&profile_id, &expected_file_hash)
                .map_err(|error| operation_error("codex-profile-delete-failed", error))
        })
        .await
    })
    .await;
    if result.is_ok() {
        if let Ok(state) = LocalState::from_app(&refresh_app) {
            if let Err(error) = crate::usage_cache::invalidate(&state, &cache_profile_id) {
                log::warn!("无法清除已删除 Codex 供应商的托盘用量缓存: {error}");
            }
            if let Err(error) = crate::usage_history::invalidate_provider(&state, &cache_profile_id)
            {
                log::warn!("无法清除已删除 Codex 供应商的用量历史: {error}");
            }
        }
        crate::tray::refresh(&refresh_app);
    }
    result
}

#[tauri::command]
pub async fn reorder_codex_profiles(
    app: tauri::AppHandle,
    ordered_ids: Vec<String>,
    expected_file_hashes: BTreeMap<String, String>,
) -> Result<(), CommandError> {
    let refresh_app = app.clone();
    let result = observe(RuntimeLogAction::ProfilesReordered, async move {
        let state = state(&app)?;
        blocking(move || {
            state
                .configuration()
                .reorder_codex_profiles(&ordered_ids, &expected_file_hashes)
                .map_err(|error| operation_error("codex-profile-reorder-failed", error))
        })
        .await
    })
    .await;
    if result.is_ok() {
        crate::tray::refresh(&refresh_app);
    }
    result
}

#[tauri::command]
pub async fn reset_profile_store(
    app: tauri::AppHandle,
    confirm_write: bool,
) -> Result<(), CommandError> {
    let refresh_app = app.clone();
    let result = observe(RuntimeLogAction::ProfileStoreReset, async move {
        error::require_write_confirmation(confirm_write, "重置供应商数据")?;
        let state = state(&app)?;
        let gateway = app
            .state::<crate::gateway::GatewayController>()
            .inner()
            .clone();
        let gate = app.state::<super::ConfigWriteGate>().inner().clone();
        blocking(move || {
            let _guard = gate
                .lock()
                .map_err(|e| CommandError::new("config-write-gate-unavailable", e))?;
            // Reset is itself the recovery path for store states the typed
            // readers reject, so recovery before the wipe is best effort: it
            // may still reconcile real client files, but no recovery failure
            // may block the confirmed wipe. Transaction backups survive under
            // state/backups for a later manual restore either way.
            if let Err(error) = switching::ensure_profile_save_recovered(&app) {
                log::warn!("重置前配置恢复未完成：{}", error.message);
            }
            if gateway.has_active_routes() {
                return Err(CommandError::new(
                    "gateway-route-active",
                    "本机协议网关正在使用供应商；请先切换到直连或官方登录后再重置供应商数据",
                ));
            }
            state
                .configuration()
                .reset()
                .map_err(|error| CommandError::new("profile-store-reset-failed", error))?;
            crate::codex_auth::clear_bindings(state.root()).map_err(|error| {
                CommandError::new(
                    "codex-bindings-reset-failed",
                    format!("供应商已重置，但账号绑定清理失败：{error}"),
                )
            })
        })
        .await
    })
    .await;
    if result.is_ok() {
        if let Ok(state) = LocalState::from_app(&refresh_app) {
            if let Err(error) = crate::usage_cache::clear(&state) {
                log::warn!("无法清除托盘用量缓存: {error}");
            }
            if let Err(error) = crate::usage_history::clear_providers(&state) {
                log::warn!("无法清除供应商用量历史: {error}");
            }
        }
        crate::codex_official_quota::clear();
        crate::tray::refresh(&refresh_app);
    }
    result
}

#[tauri::command]
pub async fn delete_profile(
    app: tauri::AppHandle,
    profile_id: String,
    expected_file_hash: String,
) -> Result<(), CommandError> {
    let refresh_app = app.clone();
    let cache_profile_id = profile_id.clone();
    let result = observe(RuntimeLogAction::ProfileDeleted, async move {
        let state = state(&app)?;
        let gateway = app
            .state::<crate::gateway::GatewayController>()
            .inner()
            .clone();
        let gate = app.state::<super::ConfigWriteGate>().inner().clone();
        blocking(move || {
            let _guard = gate
                .lock()
                .map_err(|e| CommandError::new("config-write-gate-unavailable", e))?;
            switching::ensure_profile_save_recovered(&app)?;
            if gateway.uses_profile(&profile_id) {
                return Err(CommandError::new(
                    "gateway-profile-active",
                    "该供应商正在被本机协议网关使用；请先切换到直连或官方登录后再删除",
                ));
            }
            let is_claude = state
                .configuration()
                .find_provider(&profile_id)
                .map(|profile| profile.app == AppKind::Claude)
                .unwrap_or(false);
            if !is_claude {
                crate::codex_auth::require_unbound(state.root(), &profile_id)
                    .map_err(|error| CommandError::new("codex-profile-account-bound", error))?;
            }
            let deleted = state
                .configuration()
                .delete_provider(&profile_id, &expected_file_hash)
                .map_err(|error| operation_error("profile-delete-failed", error));
            if deleted.is_ok() && is_claude {
                if let Err(error) =
                    crate::commands::failover::remove_provider_from_policy(&state, &profile_id)
                {
                    log::warn!(
                        "无法从 Claude 故障转移队列清理已删除供应商 {}: {}",
                        profile_id,
                        error
                    );
                }
                if let Err(error) = gateway.refresh_claude_candidates(&state) {
                    log::warn!("无法刷新 Claude 故障转移候选路由: {error}");
                }
            }
            deleted
        })
        .await
    })
    .await;
    if result.is_ok() {
        if let Ok(state) = LocalState::from_app(&refresh_app) {
            if let Err(error) = crate::usage_cache::invalidate(&state, &cache_profile_id) {
                log::warn!("无法清除已删除供应商的托盘用量缓存: {error}");
            }
            if let Err(error) = crate::usage_history::invalidate_provider(&state, &cache_profile_id)
            {
                log::warn!("无法清除已删除供应商的用量历史: {error}");
            }
        }
        crate::codex_official_quota::invalidate(&cache_profile_id);
        crate::tray::refresh(&refresh_app);
    }
    result
}

#[tauri::command]
pub async fn reorder_claude_profiles(
    app: tauri::AppHandle,
    ordered_ids: Vec<String>,
    expected_file_hashes: BTreeMap<String, String>,
) -> Result<(), CommandError> {
    let refresh_app = app.clone();
    let result = observe(RuntimeLogAction::ProfilesReordered, async move {
        let state = state(&app)?;
        blocking(move || {
            switching::ensure_profile_save_recovered(&app)?;
            state
                .configuration()
                .reorder_claude_providers(&ordered_ids, &expected_file_hashes)
                .map_err(|error| operation_error("claude-profile-reorder-failed", error))
        })
        .await
    })
    .await;
    if result.is_ok() {
        crate::tray::refresh(&refresh_app);
    }
    result
}

#[tauri::command]
pub async fn import_discovered_claude_profile(
    app: tauri::AppHandle,
) -> Result<ProviderRecord, CommandError> {
    let refresh_app = app.clone();
    let result = observe(RuntimeLogAction::ProfileImported, async move {
        let state = state(&app)?;
        let gateway = app
            .state::<crate::gateway::GatewayController>()
            .inner()
            .clone();
        blocking(move || {
            switching::ensure_profile_save_recovered(&app)?;
            if gateway.has_active_route_for(AppKind::Claude) {
                return Err(CommandError::new(
                    "gateway-route-active",
                    "该客户端正在使用本机协议网关；请先切换到直连或官方登录后再导入供应商",
                ));
            }
            let proposal = discovery_report()?
                .claude_import_proposals
                .into_iter()
                .next()
                .ok_or_else(|| {
                    CommandError::new("import-unavailable", "当前配置没有可导入的供应商")
                })?;
            state
                .configuration()
                .import_provider(proposal.draft)
                .map_err(|error| operation_error("profile-import-failed", error))
        })
        .await
    })
    .await;
    if result.is_ok() {
        // The imported configuration no longer matches the scanned snapshot.
        if let Ok(state) = LocalState::from_app(&refresh_app) {
            if let Err(error) = state.clear_discovery_cache() {
                log::warn!("无法清除导入后的发现扫描缓存: {error}");
            }
        }
        crate::tray::refresh(&refresh_app);
    }
    result
}

/// Imports the current local Codex route. OAuth tokens stay in auth.json and
/// third-party API keys are persisted only in the specialized Codex store.
#[tauri::command]
pub async fn import_discovered_codex_profile(
    app: tauri::AppHandle,
) -> Result<CodexImportResult, CommandError> {
    let refresh_app = app.clone();
    let result = observe(RuntimeLogAction::ProfileImported, async move {
        let state = state(&app)?;
        let gateway = app
            .state::<crate::gateway::GatewayController>()
            .inner()
            .clone();
        blocking(move || {
            switching::ensure_profile_save_recovered(&app)?;
            if gateway.has_active_route_for(AppKind::Codex) {
                return Err(CommandError::new(
                    "gateway-route-active",
                    "Codex 当前正在使用本机协议网关；请先切换到官方登录或直连后再导入供应商",
                ));
            }
            match codex_import_source()?.action {
                CodexImportAction::Official => {
                    let (record, _) = state
                        .configuration()
                        .ensure_codex_official_record()
                        .map_err(|error| operation_error("profile-import-failed", error))?;
                    Ok(CodexImportResult {
                        id: record.profile.id,
                        name: record.profile.name,
                        official: true,
                    })
                }
                CodexImportAction::ThirdParty(draft) => {
                    let record = state
                        .configuration()
                        .create_codex_provider(draft)
                        .map_err(|error| operation_error("codex-profile-import-failed", error))?;
                    Ok(CodexImportResult {
                        id: record.profile.id,
                        name: record.profile.name,
                        official: false,
                    })
                }
            }
        })
        .await
    })
    .await;
    if result.is_ok() {
        if let Ok(state) = LocalState::from_app(&refresh_app) {
            if let Err(error) = state.clear_discovery_cache() {
                log::warn!("无法清除导入后的发现扫描缓存: {error}");
            }
        }
        crate::tray::refresh(&refresh_app);
    }
    result
}
