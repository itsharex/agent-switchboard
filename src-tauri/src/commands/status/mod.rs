//! Read-only configuration status: per-client file health, route facts,
//! match classification against the profile store, and lock observation.

mod codex;
mod overview;
mod report;

#[cfg(test)]
mod parameter_tests;
#[cfg(test)]
mod tests;

use super::error::{blocking, observe, state, CommandError};
use crate::runtime_log::RuntimeLogAction;
use asb_core::contracts::{AppKind, ConfigWriteRecord, MatchStatus, RouteState, SettingsValues};
use asb_core::LockStatus;
use asb_switch::io::FsIo;
use asb_switch::lockfile::{self, RecoveryEntry};
use overview::{runtime_overview_for, runtime_transport, RuntimeOverview};
pub(crate) use report::config_status_report;
use serde::Serialize;
use tauri::{AppHandle, Manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFileStatus {
    pub app: AppKind,
    pub path: String,
    pub exists: bool,
    pub syntax_ok: bool,
    pub route: Option<RouteState>,
    pub read_error: Option<String>,
    /// Read-only client-owned values extracted from the real configuration file.
    pub client_settings: Option<SettingsValues>,
    /// A valid client file can still contain an invalid value for an owned setting.
    pub client_settings_error: Option<String>,
    pub match_status: MatchStatus,
    pub active_profile_id: Option<String>,
    pub last_switch: Option<ConfigWriteRecord>,
}

#[tauri::command]
pub async fn config_status(app: AppHandle) -> Result<Vec<ConfigFileStatus>, CommandError> {
    let state = state(&app)?;
    let gateway = app
        .state::<crate::gateway::GatewayController>()
        .inner()
        .clone();
    blocking(move || config_status_report(&state, &gateway)).await
}

#[tauri::command]
pub async fn runtime_overview(app: AppHandle) -> Result<RuntimeOverview, CommandError> {
    let app_data_dir = crate::app_paths::data_directory(&app.config().identifier)
        .map_err(|_| CommandError::new("runtime-path-unavailable", "无法定位应用数据目录"))?;
    Ok(runtime_overview_for(
        app.package_info().version.to_string(),
        &app_data_dir,
        runtime_transport(),
    ))
}

#[tauri::command]
pub async fn lock_status(app: AppHandle, target: AppKind) -> Result<LockStatus, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let target = state
            .target(target)
            .map_err(|error| CommandError::new("config-path-unavailable", error))?;
        Ok(lockfile::probe_lock(&FsIo, &target))
    })
    .await
}

#[tauri::command]
pub async fn recover_stale_lock(
    app: AppHandle,
    target: AppKind,
) -> Result<RecoveryEntry, CommandError> {
    observe(RuntimeLogAction::StaleLockRecovered, async move {
        let state = state(&app)?;
        blocking(move || {
            let target = state
                .target(target)
                .map_err(|error| CommandError::new("config-path-unavailable", error))?;
            lockfile::recover_stale(&FsIo, &target)
                .map_err(|_| CommandError::new("lock-not-stale", "当前锁不是可恢复的遗留状态"))
        })
        .await
    })
    .await
}
