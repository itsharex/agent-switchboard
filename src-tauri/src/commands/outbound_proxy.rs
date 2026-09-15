//! Outbound proxy commands: policy reads/writes plus explicit connectivity
//! checks. No client configuration is touched; the policy only gates this
//! application's own egress.

use super::error::{blocking, observe, state, CommandError};
use crate::outbound_proxy::{self, mask_url, OutboundProxyMode, OutboundProxySettings};
use crate::runtime_log::RuntimeLogAction;
use serde::Serialize;
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::time::{Duration, Instant};

fn failure(error: String) -> CommandError {
    CommandError::new("outbound-proxy-unavailable", error)
}

#[tauri::command]
pub(crate) async fn get_outbound_proxy(
    app: tauri::AppHandle,
) -> Result<OutboundProxySettings, CommandError> {
    let local = state(&app)?;
    blocking(move || outbound_proxy::read(local.root()).map_err(failure)).await
}

#[tauri::command]
pub(crate) async fn set_outbound_proxy(
    app: tauri::AppHandle,
    settings: OutboundProxySettings,
    expected_revision: String,
    confirm_write: bool,
) -> Result<OutboundProxySettings, CommandError> {
    super::error::require_write_confirmation(confirm_write, "保存出站代理设置")?;
    observe(RuntimeLogAction::OutboundProxySaved, async move {
        let local = state(&app)?;
        blocking(move || {
            // The save validates before persisting and publishes to the
            // runtime snapshot in the same call, so every later client build
            // picks the new policy up without a restart.
            outbound_proxy::save(local.root(), settings, &expected_revision).map_err(failure)
        })
        .await
    })
    .await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OutboundProxyTestResult {
    pub success: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

/// User-initiated connectivity check through the candidate URL against public
/// endpoints; the same class of request the configured upstreams will make.
#[tauri::command]
pub(crate) async fn test_outbound_proxy(
    url: String,
) -> Result<OutboundProxyTestResult, CommandError> {
    let candidate = url.trim().to_string();
    blocking(move || {
        let settings = OutboundProxySettings {
            mode: OutboundProxyMode::Manual,
            url: Some(candidate),
            ..OutboundProxySettings::default()
        };
        settings.validate().map_err(failure)
    })
    .await?;
    let started = Instant::now();
    let proxy = reqwest::Proxy::all(url.trim())
        .map_err(|error| failure(format!("代理地址无效：{error}")))?;
    let client = reqwest::Client::builder()
        .proxy(proxy)
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| failure(format!("无法构建测试客户端：{error}")))?;
    let targets = [
        "https://httpbin.org/get",
        "https://www.google.com",
        "https://api.anthropic.com",
    ];
    let mut last_error = None;
    for target in targets {
        match client.head(target).send().await {
            Ok(_) => {
                return Ok(OutboundProxyTestResult {
                    success: true,
                    latency_ms: started.elapsed().as_millis() as u64,
                    error: None,
                });
            }
            Err(error) => last_error = Some(error.to_string()),
        }
    }
    // Transport errors can echo the proxy URL; only the masked form may reach
    // the caller.
    let masked = mask_url(url.trim());
    Ok(OutboundProxyTestResult {
        success: false,
        latency_ms: started.elapsed().as_millis() as u64,
        error: last_error.map(|error| format!("{masked}：{error}")),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DetectedOutboundProxy {
    pub url: String,
    pub proxy_type: String,
    pub port: u16,
}

/// Common local proxy ports; mixed ports report both protocols.
const PROXY_PORTS: &[(u16, &str, bool)] = &[
    (7890, "http", true),
    (7891, "socks5", false),
    (1080, "socks5", false),
    (8080, "http", false),
    (8888, "http", false),
    (3128, "http", false),
    (10808, "socks5", false),
    (10809, "http", false),
];

#[tauri::command]
pub(crate) async fn scan_local_outbound_proxies(
    app: tauri::AppHandle,
) -> Result<Vec<DetectedOutboundProxy>, CommandError> {
    let _ = state(&app)?;
    tokio::task::spawn_blocking(|| {
        let mut found = Vec::new();
        for &(port, primary, mixed) in PROXY_PORTS {
            let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
            if TcpStream::connect_timeout(&address.into(), Duration::from_millis(100)).is_ok() {
                found.push(DetectedOutboundProxy {
                    url: format!("{primary}://127.0.0.1:{port}"),
                    proxy_type: primary.to_string(),
                    port,
                });
                if mixed {
                    let alternate = if primary == "http" { "socks5" } else { "http" };
                    found.push(DetectedOutboundProxy {
                        url: format!("{alternate}://127.0.0.1:{port}"),
                        proxy_type: alternate.to_string(),
                        port,
                    });
                }
            }
        }
        found
    })
    .await
    .map_err(|error| failure(format!("本地代理扫描失败：{error}")))
}
