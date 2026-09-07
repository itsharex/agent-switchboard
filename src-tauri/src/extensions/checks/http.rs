//! The HTTP(S) MCP check: modern and legacy probes, paginated listing,
//! and payload parsing.

use super::stdio::probe_for_error;
use super::stdio::{modern_supported, response_error_code, rpc_request};
use super::{
    CheckDeadline, CheckError, CheckErrorKind, McpCheckOutcome, ProbeOutcome, DISCOVERY_DEADLINE,
    LEGACY_REQUESTED_PROTOCOL_VERSION, MAX_OUTPUT_BYTES, MAX_PAGES, MODERN_PROTOCOL_VERSION,
    SUPPORTED_PROTOCOL_VERSIONS,
};
use std::collections::BTreeMap;
use std::io::Read;
struct HttpResponse {
    status: u16,
    payload: Option<serde_json::Value>,
    session: Option<String>,
}

pub(super) fn check_http(
    url: &str,
    headers: &BTreeMap<String, String>,
    bearer: Option<&str>,
    deadline: &CheckDeadline,
) -> ProbeOutcome {
    let client = match reqwest::blocking::Client::builder().build() {
        Ok(client) => client,
        Err(_) => return ProbeOutcome::failed("network", "无法创建 HTTP 检测客户端"),
    };
    let mut bytes_read = 0u64;
    match probe_modern_http(&client, url, headers, bearer, deadline, &mut bytes_read) {
        HttpModernProbe::UseModern => list_http(
            &client,
            url,
            headers,
            bearer,
            MODERN_PROTOCOL_VERSION,
            true,
            None,
            deadline,
            &mut bytes_read,
        ),
        HttpModernProbe::FallbackLegacy => {
            probe_legacy_http(&client, url, headers, bearer, deadline, &mut bytes_read)
        }
        HttpModernProbe::Outcome(outcome) => outcome,
    }
}

enum HttpModernProbe {
    UseModern,
    FallbackLegacy,
    Outcome(ProbeOutcome),
}

fn probe_modern_http(
    client: &reqwest::blocking::Client,
    url: &str,
    headers: &BTreeMap<String, String>,
    bearer: Option<&str>,
    deadline: &CheckDeadline,
    bytes_read: &mut u64,
) -> HttpModernProbe {
    let discovery_deadline = deadline.limited(DISCOVERY_DEADLINE);
    let request = rpc_request(1, "server/discover", None, true);
    let response = post_rpc(
        client,
        url,
        headers,
        bearer,
        None,
        MODERN_PROTOCOL_VERSION,
        true,
        "server/discover",
        &request,
        &discovery_deadline,
        bytes_read,
    );
    let response = match response {
        Ok(response) => response,
        Err(error)
            if matches!(error.kind, CheckErrorKind::TimedOut)
                && !deadline.cancelled()
                && deadline.remaining().is_ok() =>
        {
            return HttpModernProbe::FallbackLegacy
        }
        Err(error) => return HttpModernProbe::Outcome(probe_for_error(None, &error)),
    };
    match response.status {
        401 | 403 => {
            return HttpModernProbe::Outcome(ProbeOutcome {
                outcome: McpCheckOutcome::NeedsNativeConfirmation,
                protocol_version: None,
                truncated: false,
            })
        }
        400 | 404 | 405 | 406 => return HttpModernProbe::FallbackLegacy,
        200 => {}
        _ => {
            return HttpModernProbe::Outcome(ProbeOutcome::failed(
                "network",
                format!("HTTP {}", response.status),
            ))
        }
    }
    match response.payload.as_ref().map(modern_supported) {
        Some(Ok(true)) => HttpModernProbe::UseModern,
        Some(Ok(false)) => HttpModernProbe::FallbackLegacy,
        Some(Err(error)) => HttpModernProbe::Outcome(probe_for_error(None, &error)),
        None => HttpModernProbe::Outcome(ProbeOutcome::failed(
            "protocol",
            "server/discover 响应不是 JSON",
        )),
    }
}

fn probe_legacy_http(
    client: &reqwest::blocking::Client,
    url: &str,
    headers: &BTreeMap<String, String>,
    bearer: Option<&str>,
    deadline: &CheckDeadline,
    bytes_read: &mut u64,
) -> ProbeOutcome {
    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "initialize",
        "params": {
            "protocolVersion": LEGACY_REQUESTED_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "agent-switchboard", "version": env!("CARGO_PKG_VERSION")},
        }
    });
    let response = match post_rpc(
        client,
        url,
        headers,
        bearer,
        None,
        LEGACY_REQUESTED_PROTOCOL_VERSION,
        false,
        "initialize",
        &initialize,
        deadline,
        bytes_read,
    ) {
        Ok(response) => response,
        Err(error) => return probe_for_error(None, &error),
    };
    if matches!(response.status, 401 | 403) {
        return ProbeOutcome {
            outcome: McpCheckOutcome::NeedsNativeConfirmation,
            protocol_version: None,
            truncated: false,
        };
    }
    if response.status != 200 {
        return ProbeOutcome::failed("network", format!("HTTP {}", response.status));
    }
    let Some(payload) = response.payload else {
        return ProbeOutcome::failed("protocol", "initialize 响应不是 JSON");
    };
    let Some(result) = payload.get("result") else {
        return ProbeOutcome::failed("protocol", "服务端拒绝 initialize 请求");
    };
    let Some(protocol_version) = result
        .get("protocolVersion")
        .and_then(|value| value.as_str())
        .map(str::to_string)
    else {
        return ProbeOutcome::failed("protocol", "服务端未返回协议版本");
    };
    if !SUPPORTED_PROTOCOL_VERSIONS.contains(&protocol_version.as_str())
        || protocol_version == MODERN_PROTOCOL_VERSION
    {
        return ProbeOutcome {
            outcome: McpCheckOutcome::Failed {
                classification: "version".to_string(),
                error: format!("服务端协议版本 {protocol_version} 不在本检测支持范围内"),
            },
            protocol_version: Some(protocol_version),
            truncated: false,
        };
    }
    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    if let Err(error) = post_rpc(
        client,
        url,
        headers,
        bearer,
        response.session.as_deref(),
        &protocol_version,
        false,
        "notifications/initialized",
        &initialized,
        deadline,
        bytes_read,
    ) {
        return ProbeOutcome::partial(Some(protocol_version), error.message, false);
    }
    list_http(
        client,
        url,
        headers,
        bearer,
        &protocol_version,
        false,
        response.session.as_deref(),
        deadline,
        bytes_read,
    )
}

#[allow(clippy::too_many_arguments)]
fn list_http(
    client: &reqwest::blocking::Client,
    url: &str,
    headers: &BTreeMap<String, String>,
    bearer: Option<&str>,
    protocol_version: &str,
    modern: bool,
    session: Option<&str>,
    deadline: &CheckDeadline,
    bytes_read: &mut u64,
) -> ProbeOutcome {
    let mut counts = (0u64, 0u64, 0u64);
    for (index, method) in ["tools/list", "resources/list", "prompts/list"]
        .into_iter()
        .enumerate()
    {
        let mut cursor: Option<String> = None;
        for page in 0..MAX_PAGES {
            let request = rpc_request(
                100 + (index as u64 * MAX_PAGES as u64) + page as u64,
                method,
                cursor.as_deref(),
                modern,
            );
            let response = match post_rpc(
                client,
                url,
                headers,
                bearer,
                session,
                protocol_version,
                modern,
                method,
                &request,
                deadline,
                bytes_read,
            ) {
                Ok(response) => response,
                Err(error) if matches!(error.kind, CheckErrorKind::Cancelled) => {
                    return ProbeOutcome::cancelled(Some(protocol_version.to_string()))
                }
                Err(error) => {
                    return ProbeOutcome::partial(
                        Some(protocol_version.to_string()),
                        error.message,
                        matches!(error.kind, CheckErrorKind::OutputLimit),
                    )
                }
            };
            if matches!(response.status, 401 | 403) {
                return ProbeOutcome {
                    outcome: McpCheckOutcome::NeedsNativeConfirmation,
                    protocol_version: Some(protocol_version.to_string()),
                    truncated: false,
                };
            }
            if response.status != 200 {
                return ProbeOutcome::partial(
                    Some(protocol_version.to_string()),
                    format!("HTTP {}", response.status),
                    false,
                );
            }
            let Some(payload) = response.payload else {
                return ProbeOutcome::partial(
                    Some(protocol_version.to_string()),
                    "目录响应不是 JSON",
                    false,
                );
            };
            let Some(result) = payload.get("result") else {
                if response_error_code(&payload) == Some(-32601) {
                    break;
                }
                return ProbeOutcome::partial(
                    Some(protocol_version.to_string()),
                    "服务端拒绝目录请求",
                    false,
                );
            };
            let items = result
                .get(match method {
                    "tools/list" => "tools",
                    "resources/list" => "resources",
                    _ => "prompts",
                })
                .and_then(|value| value.as_array())
                .map(|items| items.len() as u64)
                .unwrap_or(0);
            counts = match index {
                0 => (counts.0 + items, counts.1, counts.2),
                1 => (counts.0, counts.1 + items, counts.2),
                _ => (counts.0, counts.1, counts.2 + items),
            };
            cursor = result
                .get("nextCursor")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
            if page + 1 == MAX_PAGES {
                return ProbeOutcome::partial(
                    Some(protocol_version.to_string()),
                    "目录分页超过上限，结果已截断",
                    true,
                );
            }
        }
    }
    ProbeOutcome {
        outcome: McpCheckOutcome::Passed {
            tools: counts.0,
            resources: counts.1,
            prompts: counts.2,
        },
        protocol_version: Some(protocol_version.to_string()),
        truncated: false,
    }
}

#[allow(clippy::too_many_arguments)]
fn post_rpc(
    client: &reqwest::blocking::Client,
    url: &str,
    headers: &BTreeMap<String, String>,
    bearer: Option<&str>,
    session: Option<&str>,
    protocol_version: &str,
    modern: bool,
    method: &str,
    payload: &serde_json::Value,
    deadline: &CheckDeadline,
    bytes_read: &mut u64,
) -> Result<HttpResponse, CheckError> {
    let timeout = deadline.request_timeout()?;
    let payload_bytes =
        serde_json::to_vec(payload).map_err(|_| CheckError::protocol("请求无法编码"))?;
    let mut request = client
        .post(url)
        .timeout(timeout)
        .header("Accept", "application/json, text/event-stream")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(payload_bytes);
    for (name, value) in headers {
        request = request.header(name.as_str(), value.as_str());
    }
    if let Some(session) = session {
        request = request.header("Mcp-Session-Id", session);
    }
    request = request.header("MCP-Protocol-Version", protocol_version);
    if modern {
        request = request.header("Mcp-Method", method);
    }
    if let Some(bearer) = bearer {
        request = request.bearer_auth(bearer);
    }
    let mut response = request.send().map_err(|_| {
        if deadline.cancelled() {
            CheckError::cancelled()
        } else {
            CheckError::transport("HTTP 请求失败")
        }
    })?;
    let status = response.status().as_u16();
    let session = response
        .headers()
        .get("Mcp-Session-Id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let available = MAX_OUTPUT_BYTES
        .checked_sub(*bytes_read)
        .ok_or_else(CheckError::output_limit)?;
    let mut body = Vec::new();
    response
        .by_ref()
        .take(available.saturating_add(1))
        .read_to_end(&mut body)
        .map_err(|_| CheckError::transport("HTTP 响应无法读取"))?;
    if body.len() as u64 > available {
        return Err(CheckError::output_limit());
    }
    *bytes_read = bytes_read
        .checked_add(body.len() as u64)
        .ok_or_else(CheckError::output_limit)?;
    Ok(HttpResponse {
        status,
        payload: parse_http_payload(&content_type, &body),
        session,
    })
}

fn parse_http_payload(content_type: &str, body: &[u8]) -> Option<serde_json::Value> {
    let text = std::str::from_utf8(body).ok()?;
    if content_type.contains("text/event-stream") {
        text.lines()
            .find_map(|line| line.strip_prefix("data:").map(str::trim_start))
            .and_then(|data| serde_json::from_str(data).ok())
    } else {
        serde_json::from_str(text).ok()
    }
}
