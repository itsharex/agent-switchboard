use super::error::{blocking, state, CommandError};
use crate::probe::ProbeResult;
use asb_core::contracts::{UsageQuery, UsageSummary};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProviderEndpointsRequest {
    base_url: String,
    upstream_protocol: asb_core::contracts::UpstreamProtocol,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderEndpoints {
    request_url: String,
    models_url: String,
}

/// Resolves draft addresses without reading configuration or making a request.
#[tauri::command]
pub fn resolve_provider_endpoints(
    request: ProviderEndpointsRequest,
) -> Result<ProviderEndpoints, CommandError> {
    let invalid = |error| CommandError::new("provider-endpoint-invalid", error);
    Ok(ProviderEndpoints {
        request_url: asb_core::endpoint::upstream_endpoint(
            &request.base_url,
            request.upstream_protocol,
        )
        .map_err(invalid)?,
        models_url: asb_core::endpoint::models_endpoint(
            &request.base_url,
            request.upstream_protocol,
        )
        .map_err(invalid)?,
    })
}

#[tauri::command]
pub async fn probe_endpoint(url: String) -> Result<ProbeResult, CommandError> {
    blocking(move || {
        crate::probe::probe(&url).map_err(|error| CommandError::new("probe-failed", error))
    })
    .await
}

/// Models (id plus optional vendor) from the provider's configured
/// API root's model-list endpoint. The current editor draft supplies its API key and
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
            // The provider diagnostic already redacts credentials. Generic token
            // scrubbing would erase legitimate endpoint URLs and request ids.
            .map_err(|message| CommandError {
                code: "models-fetch-failed",
                message,
            })
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

#[cfg(test)]
mod endpoint_tests {
    use super::*;

    #[test]
    fn endpoint_preview_uses_the_same_api_root_for_requests_and_model_discovery() {
        let result = resolve_provider_endpoints(ProviderEndpointsRequest {
            base_url: "https://example.test/openai/v2/".into(),
            upstream_protocol: asb_core::contracts::UpstreamProtocol::Responses,
        })
        .unwrap();
        assert_eq!(
            result.request_url,
            "https://example.test/openai/v2/responses"
        );
        assert_eq!(result.models_url, "https://example.test/openai/v2/models");
    }

    #[test]
    fn endpoint_preview_rejects_credentials_and_obsolete_endpoint_aliases() {
        for base in [
            "https://user:secret@example.test",
            "https://example.test/v1/responses",
        ] {
            let error = resolve_provider_endpoints(ProviderEndpointsRequest {
                base_url: base.into(),
                upstream_protocol: asb_core::contracts::UpstreamProtocol::Responses,
            })
            .unwrap_err();
            assert_eq!(error.code, "provider-endpoint-invalid");
            assert!(!error.message.contains("secret"));
        }
        assert!(serde_json::from_value::<ProviderEndpointsRequest>(serde_json::json!({
            "baseUrl": "https://example.test", "upstreamProtocol": "responses", "apiKey": "secret"
        })).is_err());
    }

    #[test]
    fn model_list_command_keeps_upstream_details_without_generic_scrubbing() {
        let upstream = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}/tenant/openai", upstream.server_addr());
        let server = std::thread::spawn(move || {
            let request = upstream
                .recv_timeout(std::time::Duration::from_secs(3))
                .unwrap()
                .unwrap();
            let path = request.url().to_string();
            request
                .respond(
                    tiny_http::Response::from_string(
                        "model-list access denied; isolated-model-key",
                    )
                    .with_status_code(401)
                    .with_header(
                        tiny_http::Header::from_bytes(
                            "X-Request-Id",
                            "upstream_request_abcdefghijklmnopqrstuvwxyz",
                        )
                        .unwrap(),
                    ),
                )
                .unwrap();
            path
        });
        let error = tauri::async_runtime::block_on(fetch_provider_models(ProviderModelsRequest {
            url: base.clone(),
            api_key: "isolated-model-key".into(),
            upstream_protocol: asb_core::contracts::UpstreamProtocol::Responses,
        }))
        .unwrap_err();
        assert_eq!(error.code, "models-fetch-failed");
        assert!(error.message.contains("HTTP 401"));
        assert!(error.message.contains(&format!("{base}/models")));
        assert!(error
            .message
            .contains("upstream_request_abcdefghijklmnopqrstuvwxyz"));
        assert!(error.message.contains("model-list access denied"));
        assert!(!error.message.contains("isolated-model-key"));
        assert_eq!(server.join().unwrap(), "/tenant/openai/models");
    }
}
