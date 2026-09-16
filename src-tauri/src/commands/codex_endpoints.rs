//! Codex custom endpoint commands: list/add/remove on the specialized Codex
//! file plus a credential-free reachability race across candidate URLs.
//! Nothing here writes `config.toml`; the active provider is refused so a
//! stale in-memory candidate list can never diverge from the stored file.

use super::error::{
    blocking, observe, operation_error, require_write_confirmation, state, CommandError,
};
use super::{switching, ConfigWriteGate};
use crate::runtime_log::RuntimeLogAction;
use asb_core::contracts::{AppKind, CodexProviderRecord, ProviderEndpoint};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexEndpointsView {
    pub(crate) provider_id: String,
    pub(crate) file_hash: String,
    /// The configured primary endpoint, never editable from here.
    pub(crate) primary: String,
    /// Custom targets in routing preference order (recently used first).
    pub(crate) endpoints: Vec<ProviderEndpoint>,
}

/// One row of a reachability race. Any HTTP answer counts as reachable; the
/// probe sends no model request and no credential.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexEndpointLatency {
    pub(crate) url: String,
    pub(crate) latency_ms: Option<u64>,
    pub(crate) status: Option<u16>,
    pub(crate) error: Option<String>,
}

fn view(record: CodexProviderRecord) -> CodexEndpointsView {
    let mut endpoints = record
        .profile
        .connection
        .custom_endpoints
        .into_values()
        .collect::<Vec<_>>();
    endpoints.sort_by(|left, right| {
        right
            .last_used
            .cmp(&left.last_used)
            .then_with(|| right.added_at.cmp(&left.added_at))
            .then_with(|| left.url.cmp(&right.url))
    });
    CodexEndpointsView {
        provider_id: record.profile.id,
        file_hash: record.file_hash,
        primary: record.profile.endpoint.0,
        endpoints,
    }
}

fn reject_active_provider(app: &AppHandle, provider_id: &str) -> Result<(), CommandError> {
    let Some(gateway) = app.try_state::<crate::gateway::GatewayController>() else {
        return Ok(());
    };
    if gateway.active_profile_id_for(AppKind::Codex).as_deref() == Some(provider_id) {
        return Err(CommandError::new(
            "codex-endpoint-active",
            "当前 Codex 供应商正在使用，请重新应用后再修改服务端点",
        ));
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn list_codex_endpoints(
    app: AppHandle,
    provider_id: String,
) -> Result<CodexEndpointsView, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let (file, file_hash) = state
            .configuration()
            .find_codex_provider_with_revision(&provider_id)
            .map_err(super::error::store_error)?;
        Ok(view(file.record(file_hash)))
    })
    .await
}

#[tauri::command]
pub(crate) async fn add_codex_endpoint(
    app: AppHandle,
    provider_id: String,
    url: String,
    expected_file_hash: String,
    confirm_write: bool,
) -> Result<CodexEndpointsView, CommandError> {
    require_write_confirmation(confirm_write, "添加 Codex 服务端点")?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    observe(RuntimeLogAction::ProfileUpdated, async move {
        let state = state(&app)?;
        reject_active_provider(&app, &provider_id)?;
        blocking(move || {
            let _guard = gate
                .lock()
                .map_err(|error| CommandError::new("codex-endpoint-unavailable", error))?;
            switching::ensure_profile_save_recovered(&app)?;
            state
                .configuration()
                .add_codex_endpoint(&provider_id, &url, &expected_file_hash)
                .map(view)
                .map_err(|error| operation_error("codex-endpoint-add-failed", error))
        })
        .await
    })
    .await
}

#[tauri::command]
pub(crate) async fn remove_codex_endpoint(
    app: AppHandle,
    provider_id: String,
    url: String,
    expected_file_hash: String,
    confirm_write: bool,
) -> Result<CodexEndpointsView, CommandError> {
    require_write_confirmation(confirm_write, "删除 Codex 服务端点")?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    observe(RuntimeLogAction::ProfileUpdated, async move {
        let state = state(&app)?;
        reject_active_provider(&app, &provider_id)?;
        blocking(move || {
            let _guard = gate
                .lock()
                .map_err(|error| CommandError::new("codex-endpoint-unavailable", error))?;
            switching::ensure_profile_save_recovered(&app)?;
            state
                .configuration()
                .remove_codex_endpoint(&provider_id, &url, &expected_file_hash)
                .map(view)
                .map_err(|error| operation_error("codex-endpoint-remove-failed", error))
        })
        .await
    })
    .await
}

/// Races every URL concurrently and returns rows in input order. Results
/// are advisory: the user picks what to keep; nothing is reordered here.
#[tauri::command]
pub(crate) async fn test_codex_endpoints(
    urls: Vec<String>,
) -> Result<Vec<CodexEndpointLatency>, CommandError> {
    if urls.len() > 32 {
        return Err(CommandError::new(
            "codex-endpoint-test-invalid",
            "一次最多测速 32 个端点",
        ));
    }
    blocking(move || Ok(race(&urls))).await
}

/// Per-attempt budget of one measured probe. Codex endpoints sit behind relays
/// that answer noticeably slower than the direct Claude ones, so this mirrors
/// the reference's per-app table (`codex: 12`) instead of a single flat value.
/// Kept inside the reference's [2, 30] band so a caller-supplied figure can
/// never disable the timeout entirely.
const CODEX_PROBE_TIMEOUT: Duration = Duration::from_secs(12);

fn race(urls: &[String]) -> Vec<CodexEndpointLatency> {
    std::thread::scope(|scope| {
        let handles = urls
            .iter()
            .map(|url| scope.spawn(move || measure(url)))
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .zip(urls)
            .map(|(handle, url)| {
                handle.join().unwrap_or_else(|_| CodexEndpointLatency {
                    url: url.clone(),
                    latency_ms: None,
                    status: None,
                    error: Some("测速线程异常退出".into()),
                })
            })
            .collect()
    })
}

fn measure(url: &str) -> CodexEndpointLatency {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return CodexEndpointLatency {
            url: url.to_string(),
            latency_ms: None,
            status: None,
            error: Some("端点不能为空".into()),
        };
    }
    // Warm-up first, then time one request: an untimed first hit would
    // otherwise fold DNS, TLS and the first-packet penalty into the figure
    // and make a fast endpoint look slow on its first ever measurement.
    match crate::probe::probe_warmed(trimmed, CODEX_PROBE_TIMEOUT) {
        Ok(result) => CodexEndpointLatency {
            url: trimmed.to_string(),
            latency_ms: result.latency_ms,
            status: result.status,
            error: result.error,
        },
        Err(error) => CodexEndpointLatency {
            url: trimmed.to_string(),
            latency_ms: None,
            status: None,
            error: Some(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_race_keeps_input_order_and_reports_each_url_separately() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            use std::io::{Read, Write};
            // A measuring probe spends two requests: the untimed warm-up, then
            // the timed one whose answer is reported. Answer both so the host
            // is genuinely alive for the measured phase.
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
            }
        });
        let alive = format!("http://127.0.0.1:{port}/v1");
        let rows = race(&[alive.clone(), "".into(), "ftp://nope".into()]);
        server.join().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].url, alive);
        assert_eq!(rows[0].status, Some(404), "任何 HTTP 应答都算可达");
        assert!(rows[0].latency_ms.is_some());
        assert_eq!(rows[1].error.as_deref(), Some("端点不能为空"));
        assert!(rows[2].error.is_some());
        assert_eq!(rows[2].status, None);
    }

    /// The measured phase must be the one that answers: a host that only
    /// replies to the warm-up and then goes silent reports unreachable, not a
    /// stale status borrowed from the discarded first request.
    #[test]
    fn a_host_that_only_answers_the_warm_up_is_not_reported_reachable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 1024];
            let _ = stream.read(&mut buffer);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
            // Drop the listener immediately: the timed request cannot connect.
        });
        let flaky = format!("http://127.0.0.1:{port}/v1");
        let rows = race(&[flaky.clone()]);
        server.join().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].url, flaky);
        assert_eq!(
            rows[0].status, None,
            "warm-up 的回包不得当作测量结果上报"
        );
        assert!(rows[0].error.is_some(), "{:?}", rows[0]);
    }
}
