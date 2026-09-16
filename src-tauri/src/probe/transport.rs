use asb_core::contracts::ProviderConnectionOptions;
use serde::Serialize;
use std::error::Error as _;

use std::time::{Duration, Instant};

/// Outcome grade of one manual probe: any HTTP answer proves reachability, latency above the threshold
/// grades as slow, only network-level failures (DNS / refused / TLS / timeout)
/// grade as unreachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProbeGrade {
    Ok,
    Slow,
    Unreachable,
}

/// Reachability degrades to "slow" past this TTFB, mirroring the reference
/// 6000 ms scale: probes answer in well under a second, so only genuinely
/// slow paths get flagged.
const SLOW_THRESHOLD_MS: u64 = 6_000;

pub(super) fn grade_for(latency_ms: u64) -> ProbeGrade {
    if latency_ms > SLOW_THRESHOLD_MS {
        ProbeGrade::Slow
    } else {
        ProbeGrade::Ok
    }
}

/// Result of one manual endpoint probe, surfaced to the UI as-is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub grade: ProbeGrade,
    pub status: Option<u16>,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
    /// RFC 3339 UTC timestamp of the probe.
    pub at: String,
}

/// The failure class a transport error belongs to. Retry decisions and the
/// user-facing message both key off this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FailureKind {
    Dns,
    Timeout,
    Connect,
    Tls,
    Other,
}

/// A failed transport call. The message is only the failure class — the raw
/// reqwest error text may embed the URL and is never surfaced.
#[derive(Debug, PartialEq)]
pub(super) struct ProbeFailure {
    pub(super) kind: FailureKind,
    pub(super) message: String,
}

impl ProbeFailure {
    fn is_timeout(&self) -> bool {
        self.kind == FailureKind::Timeout
    }
}

/// Maps a failure class to the message the user can act on.
pub(super) fn failure_message(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::Dns => "域名解析失败（DNS）",
        FailureKind::Timeout => "连接超时",
        FailureKind::Connect => "连接被拒绝",
        FailureKind::Tls => "TLS 握手失败",
        FailureKind::Other => "网络请求失败",
    }
}

/// The process-wide transport. The connect budget mirrors the native stack;
/// the per-attempt and per-request total budgets are applied where the calls
/// are made (see [`probe`] and [`http_request`]).
fn client() -> reqwest::blocking::Client {
    crate::outbound_proxy::cached_blocking_client("probe", |builder| {
        builder
            .user_agent("Agent Switchboard")
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("static network client configuration is valid")
    })
}

/// Total budget of one probe attempt: the 5 s connect phase plus a receive
/// phase, matching the native stack's 10 s receive timeout.
const PROBE_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);

/// Total budget of one payload-carrying request, matching the native stack's
/// 15 s send and receive timeouts.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Walks a transport error's source chain and assigns the failure class the
/// user can act on. reqwest exposes no typed DNS/TLS predicates, so the chain
/// text is matched against the stable error vocabulary of hyper/rustls/OS
/// errors; anything unrecognised lands in the generic class.
fn classify(error: &reqwest::Error) -> ProbeFailure {
    let mut texts = Vec::new();
    let mut current: Option<&dyn std::error::Error> = error.source();
    while let Some(source) = current {
        texts.push(source.to_string());
        current = source.source();
    }
    let kind = classify_kind(&texts);
    ProbeFailure {
        kind,
        message: failure_message(kind).to_string(),
    }
}

pub(super) fn classify_kind(source_texts: &[String]) -> FailureKind {
    let lower: Vec<String> = source_texts
        .iter()
        .map(|text| text.to_ascii_lowercase())
        .collect();
    let matches = |needles: &[&str]| {
        lower
            .iter()
            .any(|text| needles.iter().any(|needle| text.contains(needle)))
    };
    if error_is_timeout(source_texts) {
        return FailureKind::Timeout;
    }
    if matches(&[
        "dns error",
        "failed to lookup",
        "name or service not known",
        "nodename nor servname",
        "temporary failure in name resolution",
        "no such host",
    ]) {
        FailureKind::Dns
    } else if matches(&[
        "certificate",
        "invalid peer",
        "unknown issuer",
        "tls",
        "ssl",
    ]) {
        FailureKind::Tls
    } else if matches(&[
        "refused",
        "unreachable",
        "reset by peer",
        "tcp connect",
        "connect error",
        "client error (connect)",
    ]) {
        FailureKind::Connect
    } else {
        FailureKind::Other
    }
}

fn error_is_timeout(source_texts: &[String]) -> bool {
    source_texts.iter().any(|text| {
        let text = text.to_ascii_lowercase();
        text.contains("timed out") || text.contains("operation was canceled due to timeouts")
    })
}

/// Probe retry policy, separated from the transport layer for testing:
/// timeout-class failures get exactly one retry (network jitter), immediate
/// failures such as refused connections or DNS errors fail fast.
pub(super) fn probe_with_retries(
    mut attempt: impl FnMut() -> Result<u16, ProbeFailure>,
) -> Result<u16, ProbeFailure> {
    match attempt() {
        Err(failure) if failure.is_timeout() => attempt(),
        first => first,
    }
}

pub(super) struct ParsedUrl {
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) path: String,
    pub(super) secure: bool,
}

/// Splits a validated `http(s)://` URL into scheme, authority and path.
pub(super) fn parse_url(url: &str) -> Option<ParsedUrl> {
    let (secure, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        (false, rest)
    } else {
        return None;
    };
    let split = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..split];
    let mut path = rest[split..].to_string();
    if path.is_empty() || path.starts_with('?') || path.starts_with('#') {
        path.insert(0, '/');
    }
    // IPv6 literals keep their brackets out of the port split.
    let (host, port) = if let Some(close) = authority.find(']') {
        let host = authority[..=close].to_string();
        let tail = &authority[close + 1..];
        let port = tail
            .strip_prefix(':')
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(if secure { 443 } else { 80 });
        (host, port)
    } else {
        match authority.rsplit_once(':') {
            Some((host, digits))
                if !host.is_empty()
                    && !digits.is_empty()
                    && digits.bytes().all(|b| b.is_ascii_digit()) =>
            {
                (host.to_string(), digits.parse::<u16>().ok()?)
            }
            _ => (authority.to_string(), if secure { 443 } else { 80 }),
        }
    };
    if host.is_empty() {
        return None;
    }
    Some(ParsedUrl {
        host,
        port,
        path,
        secure,
    })
}

fn request_url(parsed: &ParsedUrl) -> String {
    let scheme = if parsed.secure { "https" } else { "http" };
    format!("{scheme}://{}:{}{}", parsed.host, parsed.port, parsed.path)
}

/// Probes one URL for reachability. Any HTTP answer counts — the status code
/// (200/4xx/5xx alike) only proves the endpoint is alive; the probe sends no
/// model request and carries no credential, so it never validates
/// authentication or model configuration. The response body is never read.
pub fn probe(url: &str) -> Result<ProbeResult, String> {
    probe_with_client(url, &client())
}

/// Probes one URL the way an endpoint race measures it: one untimed warm-up
/// request first, so the connection is reused and the recorded figure is not
/// inflated by the first-packet penalty, then a single timed request whose
/// result is reported. `attempt_timeout` is applied to both phases so a dead
/// host cannot hold the race open longer than the caller allowed.
///
/// The warm-up is deliberately untimed and its outcome discarded: a host that
/// only answers the second request is still reported from the timed phase, and
/// a host that fails both still surfaces exactly one classified error.
pub fn probe_warmed(url: &str, attempt_timeout: Duration) -> Result<ProbeResult, String> {
    measure(url, &client(), attempt_timeout, true)
}

pub(super) fn probe_with_client(
    url: &str,
    client: &reqwest::blocking::Client,
) -> Result<ProbeResult, String> {
    measure(url, client, PROBE_ATTEMPT_TIMEOUT, false)
}

/// The single measurement body behind both probe entry points: they differ
/// only in the attempt budget and whether an untimed warm-up runs first.
fn measure(
    url: &str,
    client: &reqwest::blocking::Client,
    attempt_timeout: Duration,
    warm_up: bool,
) -> Result<ProbeResult, String> {
    let parsed = parse_url(url).ok_or_else(|| "端点必须是 http(s) URL".to_string())?;
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let url = request_url(&parsed);

    if warm_up {
        // Outcome intentionally ignored: no retry, no timing.
        let _ = client.get(&url).timeout(attempt_timeout).send();
    }

    let started = Instant::now();
    let outcome = probe_with_retries(|| {
        client
            .get(&url)
            .timeout(attempt_timeout)
            .send()
            .map(|response| response.status().as_u16())
            .map_err(|error| classify(&error))
    });
    let latency_ms = Some(started.elapsed().as_millis() as u64);
    Ok(match outcome {
        Ok(status) => ProbeResult {
            grade: grade_for(latency_ms.unwrap_or(0)),
            status: Some(status),
            latency_ms,
            error: None,
            at,
        },
        Err(failure) => ProbeResult {
            grade: ProbeGrade::Unreachable,
            status: None,
            latency_ms,
            error: Some(failure.message),
            at,
        },
    })
}

/// Parses the CRLF-joined request-header convention into a header map. An
/// empty string maps to no headers; anything malformed is rejected loudly
/// instead of silently dropped.
fn header_map(raw: &str) -> Result<reqwest::header::HeaderMap, String> {
    let mut map = reqwest::header::HeaderMap::new();
    for line in raw.split("\r\n") {
        if line.trim().is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| "请求头无效".to_string())?;
        let name = reqwest::header::HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| "请求头无效".to_string())?;
        let value = reqwest::header::HeaderValue::from_bytes(value.trim().as_bytes())
            .map_err(|_| "请求头无效".to_string())?;
        map.insert(name, value);
    }
    Ok(map)
}

/// One request returning the response status and body text. The shared
/// client doubles as the required `User-Agent`; callers pass extra request
/// headers as CRLF-joined lines. The whole request is budgeted at 15 s.
pub fn http_request(
    method: &str,
    url: &str,
    headers: &str,
    body: &[u8],
) -> Result<(u16, String), String> {
    http_request_with_options(
        method,
        url,
        headers,
        body,
        &ProviderConnectionOptions::default(),
    )
}

/// Variant of [`http_request`] that applies provider-owned request headers
/// and User-Agent overrides after the query has built its own request.
pub fn http_request_with_options(
    method: &str,
    url: &str,
    headers: &str,
    body: &[u8],
    connection: &ProviderConnectionOptions,
) -> Result<(u16, String), String> {
    let parsed = parse_url(url).ok_or_else(|| "请求地址必须是 http(s) URL".to_string())?;
    let method =
        reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| "请求方式无效".to_string())?;
    let mut headers = header_map(headers)?;
    crate::upstream_overrides::apply_header_overrides(&mut headers, connection);

    let mut request = client()
        .request(method, request_url(&parsed))
        .timeout(REQUEST_TIMEOUT)
        .headers(headers);
    if !body.is_empty() {
        request = request.body(body.to_vec());
    }
    let response = request.send().map_err(|error| classify(&error).message)?;
    let status = response.status().as_u16();
    let bytes = response.bytes().map_err(|error| classify(&error).message)?;
    Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
}

/// Convenience wrapper for the existing outbound read-only callers.
pub fn http_get(url: &str, headers: &str) -> Result<(u16, String), String> {
    http_request("GET", url, headers, &[])
}

/// GET variant that applies the provider's request-header and User-Agent
/// overrides without exposing a host I/O capability to usage scripts.
pub fn http_get_with_options(
    url: &str,
    headers: &str,
    connection: &ProviderConnectionOptions,
) -> Result<(u16, String), String> {
    http_request_with_options("GET", url, headers, &[], connection)
}
