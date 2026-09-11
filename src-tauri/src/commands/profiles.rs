use super::discovery::discovery_report;
use super::error::{self, blocking, observe, operation_error, state, store_error, CommandError};
use super::switching;
use crate::local_state::LocalState;
use crate::runtime_log::RuntimeLogAction;
use asb_core::contracts::{AppKind, CodexProviderDraft, CodexProviderRecord, ProviderRecord};
use std::collections::BTreeMap;
use tauri::Manager;

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
) -> Result<Vec<CodexProviderRecord>, CommandError> {
    let refresh_app = app.clone();
    let result = observe(RuntimeLogAction::ProfilesReordered, async move {
        let state = state(&app)?;
        blocking(move || {
            state
                .configuration()
                .reorder_codex_providers(&ordered_ids, &expected_file_hashes)
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
        blocking(move || {
            switching::ensure_profile_save_recovered(&app)?;
            if gateway.has_active_routes() {
                return Err(CommandError::new(
                    "gateway-route-active",
                    "本机协议网关正在使用供应商；请先切换到直连或官方登录后再重置供应商数据",
                ));
            }
            state
                .configuration()
                .reset()
                .map_err(|error| CommandError::new("profile-store-reset-failed", error))
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
        blocking(move || {
            switching::ensure_profile_save_recovered(&app)?;
            if gateway.uses_profile(&profile_id) {
                return Err(CommandError::new(
                    "gateway-profile-active",
                    "该供应商正在被本机协议网关使用；请先切换到直连或官方登录后再删除",
                ));
            }
            state
                .configuration()
                .delete_provider(&profile_id, &expected_file_hash)
                .map_err(|error| operation_error("profile-delete-failed", error))
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
pub async fn reorder_profiles(
    app: tauri::AppHandle,
    target: AppKind,
    ordered_ids: Vec<String>,
    expected_file_hashes: BTreeMap<String, String>,
) -> Result<Vec<ProviderRecord>, CommandError> {
    let refresh_app = app.clone();
    let result = observe(RuntimeLogAction::ProfilesReordered, async move {
        let state = state(&app)?;
        blocking(move || {
            switching::ensure_profile_save_recovered(&app)?;
            state
                .configuration()
                .reorder_providers(target, &ordered_ids, &expected_file_hashes)
                .map_err(|error| operation_error("profile-reorder-failed", error))
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
