//! The single owner of scheduled provider-usage re-queries.
//!
//! A daemon thread walks every profile's configured `refresh_interval_minutes`
//! once a minute and re-queries the due ones. Both this thread and the manual
//! command execute through `execute_once`, so every attempt — successful or
//! not — advances the persisted timing in the usage cache.

use crate::config_store::StoreOperationError;
use crate::local_state::LocalState;
use crate::tray;
use crate::usage_cache;
use asb_core::contracts::{ProviderProfile, UsageSummary};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

/// One scheduler tick. The smallest configured cadence is one minute, so a
/// minute-aligned walk reaches every profile at its own interval.
const TICK: Duration = Duration::from_secs(60);

pub(crate) fn spawn(app: AppHandle) {
    let spawned = thread::Builder::new()
        .name("usage-scheduler".into())
        .spawn(move || loop {
            run_tick(&app);
            thread::sleep(TICK);
        });
    if let Err(error) = spawned {
        log::warn!("无法启动用量定时查询线程: {error}");
    }
}

fn run_tick(app: &AppHandle) {
    let state = match LocalState::from_app(app) {
        Ok(state) => state,
        Err(error) => {
            log::warn!("定时用量查询跳过一轮：{error}");
            return;
        }
    };
    let records = match state.configuration().list_providers() {
        Ok(records) => records,
        Err(error) => {
            log::warn!("定时用量查询无法读取供应商列表：{error}");
            return;
        }
    };
    let mut profiles = records
        .into_iter()
        .map(|record| record.profile)
        .collect::<Vec<_>>();
    let codex_records = match state.configuration().list_codex_providers() {
        Ok(records) => records,
        Err(error) => {
            log::warn!("定时用量查询无法读取 Codex 供应商列表：{error}");
            return;
        }
    };
    for record in codex_records {
        match state
            .configuration()
            .find_codex_provider_file(&record.profile.id)
        {
            Ok(file) => profiles.push(
                file.client_projection()
                    .into_profile(asb_core::contracts::AppKind::Codex),
            ),
            Err(error) => log::warn!("定时用量查询无法读取 Codex 供应商：{error}"),
        }
    }
    for profile in &profiles {
        let Some(query) = profile.usage_query.as_ref() else {
            continue;
        };
        let interval = query.refresh_interval_minutes();
        if interval == 0 || !usage_cache::due(&state, profile, interval, chrono::Utc::now()) {
            continue;
        }
        match execute_once(&state, profile) {
            Ok(_) => tray::refresh(app),
            Err(error) => log::warn!(
                "供应商 {} 的定时用量查询失败：{}",
                profile.name,
                asb_core::adapter::scrub_message(error)
            ),
        }
    }
}

/// Runs one persisted query and records its outcome. The API key stays inside
/// this backend boundary; only the credential-free summary leaves it.
pub(crate) fn execute_once(
    state: &LocalState,
    profile: &ProviderProfile,
) -> Result<UsageSummary, String> {
    let query = profile
        .usage_query
        .clone()
        .ok_or_else(|| "该供应商尚未配置用量查询".to_string())?;
    let upstream_protocol = profile
        .upstream_protocol
        .ok_or_else(|| "供应商缺少上游 API 格式".to_string())?;
    match crate::usage_query::run_usage_query_with_connection(
        &query,
        &profile.api_key,
        profile.base_url.as_deref(),
        upstream_protocol,
        profile.authentication,
        &profile.connection,
    ) {
        Ok(summary) => {
            usage_cache::record_success(state, profile, summary.clone())?;
            if let Err(error) = crate::usage_history::record_provider(state, profile, &summary) {
                log::warn!("用量查询成功，但无法保存趋势历史: {error}");
            }
            Ok(summary)
        }
        Err(error) => {
            if let Err(cache_error) = usage_cache::record_failure(state, profile) {
                log::warn!("用量查询失败，且无法记录尝试时间: {cache_error}");
            }
            Err(error)
        }
    }
}

/// Resolves the credential-bearing query input at the backend boundary.
/// Codex uses its specialized persisted file and only borrows the generic
/// client projection transiently for the shared usage-query implementation.
pub(crate) fn usage_profile(
    state: &LocalState,
    profile_id: &str,
) -> Result<ProviderProfile, StoreOperationError> {
    if state
        .configuration()
        .list_codex_providers()?
        .into_iter()
        .any(|record| record.profile.id == profile_id)
    {
        return state
            .configuration()
            .find_codex_provider_file(profile_id)
            .map(|file| {
                file.client_projection()
                    .into_profile(asb_core::contracts::AppKind::Codex)
            })
            .map_err(StoreOperationError::from);
    }
    state.configuration().find_provider(profile_id)
}
