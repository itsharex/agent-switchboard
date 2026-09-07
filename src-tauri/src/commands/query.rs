use super::error::{blocking, state, CommandError};
use crate::probe::ProbeResult;
use asb_core::contracts::{UsageQuery, UsageSummary};
use serde::Deserialize;

#[tauri::command]
pub async fn probe_endpoint(url: String) -> Result<ProbeResult, CommandError> {
    blocking(move || {
        crate::probe::probe(&url).map_err(|error| CommandError::new("probe-failed", error))
    })
    .await
}

/// Models (id plus optional vendor) from the provider's configured
/// `/v1/models` endpoint. The current editor draft supplies its API key and
/// protocol; the backend derives the request header from that protocol. The
/// key is never included in errors or persisted by this command.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProviderModelsRequest {
    url: String,
    api_key: String,
    upstream_protocol: asb_core::contracts::UpstreamProtocol,
}

#[tauri::command]
pub async fn fetch_provider_models(
    request: ProviderModelsRequest,
) -> Result<Vec<crate::probe::ProviderModel>, CommandError> {
    blocking(move || {
        crate::probe::fetch_models(&request.url, &request.api_key, request.upstream_protocol)
            .map_err(|error| CommandError::new("models-fetch-failed", error))
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UsageQueryRequest {
    query: UsageQuery,
    api_key: String,
    base_url: Option<String>,
    upstream_protocol: asb_core::contracts::UpstreamProtocol,
}

#[tauri::command]
pub async fn test_usage_query(request: UsageQueryRequest) -> Result<UsageSummary, CommandError> {
    blocking(move || {
        crate::usage_query::run_usage_query(
            &request.query,
            &request.api_key,
            request.base_url.as_deref(),
            request.upstream_protocol,
        )
        .map_err(|error| CommandError::new("usage-query-failed", error))
    })
    .await
}

/// Runs the persisted query of one provider and records the successful
/// credential-free summary for the custom tray panel. This is the manual,
/// immediate path; the scheduled path lives in `usage_query::scheduler` and
/// shares the same executor. The renderer passes only the stable profile id;
/// this backend boundary owns the query and API key.
#[tauri::command]
pub async fn query_profile_usage(
    app: tauri::AppHandle,
    profile_id: String,
) -> Result<UsageSummary, CommandError> {
    let state = state(&app)?;
    let summary = blocking(move || {
        let profile = state
            .configuration()
            .find_provider(&profile_id)
            .map_err(|error| CommandError::new("profile-not-found", error))?;
        crate::usage_query::scheduler::execute_once(&state, &profile)
            .map_err(|error| CommandError::new("usage-query-failed", error))
    })
    .await?;
    crate::tray::refresh(&app);
    Ok(summary)
}

/// Reads the last successful summary of one profile from the tray cache
/// without contacting the provider. `null` means no successful query exists
/// for the profile's current usage query.
#[tauri::command]
pub async fn read_profile_usage(
    app: tauri::AppHandle,
    profile_id: String,
) -> Result<Option<UsageSummary>, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let profile = state
            .configuration()
            .find_provider(&profile_id)
            .map_err(|error| CommandError::new("profile-not-found", error))?;
        Ok(crate::usage_cache::get(&state, &profile))
    })
    .await
}
