//! The single owner of scheduled provider-usage re-queries.
//!
//! A daemon thread keeps a fixed 60-second cadence anchored at each tick's
//! start: it sleeps for whatever remains of the tick, and a tick that
//! overruns starts the next immediately. Every profile's interval is measured
//! from its attempt's initiation — stamped before the network call — and due
//! profiles are queried in parallel, so no slow endpoint lengthens any
//! profile's cadence. The scheduler, the forced manual query, and the
//! demand-driven ensure-fresh read all execute through `execute_once`, so
//! every attempt — successful or not — advances the persisted timing in the
//! usage cache.

use crate::config_store::StoreOperationError;
use crate::local_state::LocalState;
use crate::tray;
use crate::usage_cache;
use asb_core::contracts::{ProviderProfile, UsageSummary};
use std::thread;
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// One scheduler tick. The loop sleeps for the time left in the tick after
/// its work; the smallest configured cadence is one minute, so every profile
/// keeps its own interval regardless of how long the work took.
const TICK: Duration = Duration::from_secs(60);

pub(crate) fn spawn(app: AppHandle) {
    let spawned = thread::Builder::new()
        .name("usage-scheduler".into())
        .spawn(move || loop {
            let tick_started = Instant::now();
            run_tick(&app);
            let elapsed = tick_started.elapsed();
            if elapsed < TICK {
                thread::sleep(TICK - elapsed);
            }
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
    let now = chrono::Utc::now();
    let mut due = Vec::new();
    for profile in profiles {
        let Some(query) = profile.usage_query.as_ref() else {
            continue;
        };
        let interval = query.refresh_interval_minutes();
        if interval != 0 && usage_cache::due(&state, &profile, interval, now) {
            due.push(profile);
        }
    }
    // Each due profile queries on its own thread so one endpoint cannot delay
    // another; scope joins them, and the transport's bounded timeouts keep
    // every thread finite.
    thread::scope(|scope| {
        for profile in due {
            scope.spawn(move || {
                let Ok(state) = LocalState::from_app(app) else {
                    log::warn!("定时用量查询无法读取本地状态");
                    return;
                };
                match execute_once(&state, &profile) {
                    Ok(_) => tray::refresh(app),
                    Err(error) => log::warn!(
                        "供应商 {} 的定时用量查询失败：{}",
                        profile.name,
                        asb_core::adapter::scrub_message(error)
                    ),
                }
            });
        }
    });
}

/// Runs one persisted query and records its outcome. The attempt's timing
/// baseline is its initiation, stamped before the network call, so a slow
/// response never lengthens the profile's cadence. The API key stays inside
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
    let attempted_at = chrono::Utc::now();
    match crate::usage_query::run_usage_query_with_connection(
        &query,
        &profile.api_key,
        profile.base_url.as_deref(),
        upstream_protocol,
        profile.authentication,
        &profile.connection,
    ) {
        Ok(summary) => {
            usage_cache::record_success(state, profile, summary.clone(), attempted_at)?;
            if let Err(error) = crate::usage_history::record_provider(state, profile, &summary) {
                log::warn!("用量查询成功，但无法保存趋势历史: {error}");
            }
            Ok(summary)
        }
        Err(error) => {
            if let Err(cache_error) = usage_cache::record_failure(state, profile, attempted_at) {
                log::warn!("用量查询失败，且无法记录尝试时间: {cache_error}");
            }
            Err(error)
        }
    }
}

/// Returns the profile's cached summary while it is fresh, or pulls one
/// query forward into now. The scheduler stays the only automatic
/// re-querier: demand may advance a due query, never defer one.
pub(crate) fn ensure_fresh(
    state: &LocalState,
    profile: &ProviderProfile,
) -> Result<(UsageSummary, bool), String> {
    let interval = profile
        .usage_query
        .as_ref()
        .map(|query| query.refresh_interval_minutes())
        .unwrap_or(0);
    if let Some(cached) = usage_cache::get(state, profile) {
        if !usage_cache::due(state, profile, interval, chrono::Utc::now()) {
            return Ok((cached, false));
        }
    }
    execute_once(state, profile).map(|summary| (summary, true))
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
