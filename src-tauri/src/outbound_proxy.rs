//! Outbound proxy policy for every gateway-side HTTP client.
//!
//! One owner of the egress contract: none (always direct), system (follow the
//! environment, minus a self-loop guard) or manual (an explicit http/https/
//! socks5/socks5h URL, credentials included). Gateway upstreams, probes and
//! provider requests all build through [`apply`] so a settings change is
//! picked up on the next client build; the revision-cached client registry
//! makes that immediate for long-lived holders.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

const VERSION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum OutboundProxyMode {
    #[default]
    None,
    System,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OutboundProxySettings {
    pub version: u64,
    pub mode: OutboundProxyMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub revision: String,
}
impl Default for OutboundProxySettings {
    fn default() -> Self {
        Self {
            version: VERSION,
            mode: OutboundProxyMode::None,
            url: None,
            revision: uuid::Uuid::nil().to_string(),
        }
    }
}
impl OutboundProxySettings {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.version != VERSION {
            return Err("出站代理设置版本不受支持".into());
        }
        if uuid::Uuid::parse_str(&self.revision).is_err() {
            return Err("出站代理设置修订无效".into());
        }
        match self.mode {
            OutboundProxyMode::None => {
                if self.url.is_some() {
                    return Err("直连模式不能携带代理地址".into());
                }
            }
            OutboundProxyMode::System => {
                if self.url.is_some() {
                    return Err("系统代理模式不能携带代理地址".into());
                }
            }
            OutboundProxyMode::Manual => {
                let Some(url) = self.url.as_deref().map(str::trim).filter(|u| !u.is_empty()) else {
                    return Err("手动代理需要代理地址".into());
                };
                if url.len() > 2048 || url.chars().any(char::is_control) {
                    return Err("代理地址过长或含有控制字符".into());
                }
                // This module owns the accepted scheme set; reqwest then owns
                // the rest of the grammar (host, port, credentials).
                const SCHEMES: [&str; 4] = ["http", "https", "socks5", "socks5h"];
                let scheme = url.split("://").next().unwrap_or_default();
                if !SCHEMES.contains(&scheme) {
                    return Err(format!(
                        "代理协议必须是 http、https、socks5 或 socks5h：{scheme}"
                    ));
                }
                reqwest::Proxy::all(url).map_err(|error| format!("代理地址无效：{error}"))?;
            }
        }
        Ok(())
    }
}

/// Masks credentials in a proxy URL for logs and diagnostics.
#[expect(dead_code)] // reserved: proxy log surface
pub(crate) fn mask_url(url: &str) -> String {
    let Some(position) = url.find("://") else {
        return "[代理]".into();
    };
    let (scheme, rest) = url.split_at(position + 3);
    match rest.rsplit_once('@') {
        Some((_, host)) => format!("{scheme}***@{host}"),
        None => url.into(),
    }
}

pub(crate) fn read(state_root: &Path) -> Result<OutboundProxySettings, String> {
    let path = settings_path(state_root);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(OutboundProxySettings::default());
        }
        Err(error) => return Err(format!("无法读取出站代理设置：{error}")),
    };
    let settings: OutboundProxySettings =
        serde_json::from_slice(&bytes).map_err(|_| "出站代理设置文件无效".to_string())?;
    settings.validate()?;
    Ok(settings)
}

#[expect(dead_code)] // reserved: proxy settings editor
pub(crate) fn save(
    state_root: &Path,
    settings: OutboundProxySettings,
    expected_revision: &str,
) -> Result<OutboundProxySettings, String> {
    settings.validate()?;
    let current = read(state_root)?;
    if current.revision != expected_revision {
        return Err("出站代理设置已改变，请重新读取后再保存".into());
    }
    if current == settings {
        return Ok(current);
    }
    let mut saved = settings;
    saved.revision = uuid::Uuid::new_v4().to_string();
    let path = settings_path(state_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("无法创建设置目录：{error}"))?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec_pretty(&saved).map_err(|_| "出站代理设置无法编码".to_string())?,
    )
    .map_err(|error| format!("无法写取出站代理设置：{error}"))?;
    std::fs::rename(&temporary, &path).map_err(|error| format!("无法提交出站代理设置：{error}"))?;
    store_snapshot(Arc::new(Snapshot {
        revision: saved.revision.clone(),
        mode: saved.mode,
        url: saved.url.clone(),
    }));
    Ok(saved)
}

fn settings_path(state_root: &Path) -> PathBuf {
    state_root.join("outbound-proxy.json")
}

// ---------------------------------------------------------------------------
// Runtime policy
// ---------------------------------------------------------------------------

struct Snapshot {
    revision: String,
    mode: OutboundProxyMode,
    url: Option<String>,
}

fn cell() -> &'static RwLock<Arc<Snapshot>> {
    static SNAPSHOT: OnceLock<RwLock<Arc<Snapshot>>> = OnceLock::new();
    SNAPSHOT.get_or_init(|| {
        RwLock::new(Arc::new(Snapshot {
            revision: OutboundProxySettings::default().revision,
            mode: OutboundProxyMode::None,
            url: None,
        }))
    })
}

fn snapshot() -> Arc<Snapshot> {
    cell()
        .read()
        .map(|current| current.clone())
        .unwrap_or_else(|_| {
            Arc::new(Snapshot {
                revision: OutboundProxySettings::default().revision,
                mode: OutboundProxyMode::None,
                url: None,
            })
        })
}

fn store_snapshot(next: Arc<Snapshot>) {
    if let Ok(mut current) = cell().write() {
        *current = next;
    }
}

/// Loads persisted settings into the process snapshot. Called at startup and
/// by the settings command after a save; reading the file lazily per client
/// build would race concurrent writes.
pub(crate) fn load(state_root: &Path) -> Result<(), String> {
    let settings = read(state_root)?;
    store_snapshot(Arc::new(Snapshot {
        revision: settings.revision.clone(),
        mode: settings.mode,
        url: settings.url.clone(),
    }));
    Ok(())
}

fn port_cell() -> &'static RwLock<Option<u16>> {
    static PORT: OnceLock<RwLock<Option<u16>>> = OnceLock::new();
    PORT.get_or_init(|| RwLock::new(None))
}

fn gateway_port() -> Option<u16> {
    port_cell().read().ok().and_then(|port| *port)
}

/// The gateway records its listening port so System mode can refuse to egress
/// through itself.
pub(crate) fn note_gateway_port(port: u16) {
    if let Ok(mut current) = port_cell().write() {
        *current = Some(port);
    }
}

fn system_proxy_points_at_self() -> bool {
    let Some(port) = gateway_port() else {
        return false;
    };
    let mut variables = Vec::new();
    for name in ["http_proxy", "https_proxy", "all_proxy"] {
        variables.push(std::env::var(name).or_else(|_| std::env::var(name.to_uppercase())));
    }
    variables
        .into_iter()
        .flatten()
        .filter_map(|value| reqwest::Url::parse(value.trim()).ok())
        .any(|url| {
            let host_is_loopback = url
                .host_str()
                .is_some_and(|host| host.eq_ignore_ascii_case("localhost"))
                || url
                    .host_str()
                    .is_some_and(|host| matches!(host, "127.0.0.1" | "[::1]" | "::1" | "0.0.0.0"));
            host_is_loopback && url.port() == Some(port)
        })
}

/// The resolved egress policy; pure so tests can pin it without sockets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EgressDecision {
    /// Force direct egress regardless of the environment.
    Direct,
    /// Follow the environment/system proxy configuration.
    System,
    /// Route through this explicit proxy URL.
    Proxy(String),
}

fn decision_for(current: &Snapshot) -> EgressDecision {
    match current.mode {
        OutboundProxyMode::None => EgressDecision::Direct,
        OutboundProxyMode::System => {
            if system_proxy_points_at_self() {
                EgressDecision::Direct
            } else {
                EgressDecision::System
            }
        }
        OutboundProxyMode::Manual => match current
            .url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
        {
            Some(url) => EgressDecision::Proxy(url.to_string()),
            None => EgressDecision::Direct,
        },
    }
}

/// Applies the current policy to an async client builder. `None` forces direct
/// egress; `System` keeps reqwest's environment handling unless the detected
/// proxy is this gateway itself (a request loop); `Manual` routes everything
/// through the configured proxy.
pub(crate) fn apply(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    match decision_for(&snapshot()) {
        EgressDecision::Direct => builder.no_proxy(),
        EgressDecision::System => builder,
        EgressDecision::Proxy(url) => match reqwest::Proxy::all(&url) {
            Ok(proxy) => builder.proxy(proxy),
            Err(_) => builder.no_proxy(),
        },
    }
}

/// Blocking twin of [`apply`] for the probe transport.
pub(crate) fn apply_blocking(
    builder: reqwest::blocking::ClientBuilder,
) -> reqwest::blocking::ClientBuilder {
    match decision_for(&snapshot()) {
        EgressDecision::Direct => builder.no_proxy(),
        EgressDecision::System => builder,
        EgressDecision::Proxy(url) => match reqwest::Proxy::all(&url) {
            Ok(proxy) => builder.proxy(proxy),
            Err(_) => builder.no_proxy(),
        },
    }
}

/// Blocking twin of [`cached_client` for the probe transport.
pub(crate) fn cached_blocking_client(
    key: &'static str,
    build: impl FnOnce(reqwest::blocking::ClientBuilder) -> reqwest::blocking::Client,
) -> reqwest::blocking::Client {
    static CLIENTS: OnceLock<RwLock<Vec<(&'static str, String, reqwest::blocking::Client)>>> =
        OnceLock::new();
    let cell = CLIENTS.get_or_init(|| RwLock::new(Vec::new()));
    let current = snapshot();
    if let Ok(clients) = cell.read() {
        for (existing, revision, client) in clients.iter() {
            if *existing == key && *revision == current.revision {
                return client.clone();
            }
        }
    }
    let client = build(apply_blocking(reqwest::blocking::ClientBuilder::new()));
    if let Ok(mut clients) = cell.write() {
        clients.retain(|(existing, revision, _)| *existing != key || *revision == current.revision);
        clients.push((key, current.revision.clone(), client.clone()));
    }
    client
}

/// Revision-keyed client registry: long-lived holders re-fetch by key and get
/// a fresh client exactly when the policy revision changed.
pub(crate) fn cached_client(
    key: &'static str,
    build: impl FnOnce(reqwest::ClientBuilder) -> reqwest::Client,
) -> reqwest::Client {
    static CLIENTS: OnceLock<RwLock<Vec<(&'static str, String, reqwest::Client)>>> =
        OnceLock::new();
    let cell = CLIENTS.get_or_init(|| RwLock::new(Vec::new()));
    let current = snapshot();
    if let Ok(clients) = cell.read() {
        for (existing, revision, client) in clients.iter() {
            if *existing == key && *revision == current.revision {
                return client.clone();
            }
        }
    }
    let client = build(apply(reqwest::Client::builder()));
    if let Ok(mut clients) = cell.write() {
        clients.retain(|(existing, revision, _)| *existing != key || *revision == current.revision);
        clients.push((key, current.revision.clone(), client.clone()));
    }
    client
}

