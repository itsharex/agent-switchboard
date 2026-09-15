//! Claude 官方登录订阅额度：只读取 Claude CLI 自己的登录缓存并查询官方用量
//! 接口。不刷新、不改写凭据，也不参与托管账号（`claude_auth`）或请求账本；
//! 与 Codex 官方额度实现完全分开。

use serde::Serialize;
use serde_json::Value;
use std::path::Path;

pub(crate) const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";
/// Known rolling windows and their display labels, in upstream order.
const KNOWN_WINDOWS: [(&str, &str); 4] = [
    ("five_hour", "5 小时"),
    ("seven_day", "7 天"),
    ("seven_day_opus", "7 天（Opus）"),
    ("seven_day_sonnet", "7 天（Sonnet）"),
];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeNativeQuota {
    pub windows: Vec<ClaudeNativeQuotaWindow>,
    pub extra_usage: Option<ClaudeNativeExtraUsage>,
    pub credential_expires_at_ms: Option<i64>,
    /// The cached token looked expired before the query; the CLI refreshes
    /// its own cache, so a successful answer here is still authoritative.
    pub credential_expired: bool,
    pub checked_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeNativeQuotaWindow {
    pub id: String,
    pub label: String,
    pub used_percent: f64,
    pub resets_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeNativeExtraUsage {
    pub enabled: bool,
    pub monthly_limit: Option<f64>,
    pub used_credits: Option<f64>,
    pub utilization: Option<f64>,
    pub currency: Option<String>,
}

/// The one credential read for a query. Never serialized or logged.
struct Credential {
    access_token: String,
    expires_at_ms: Option<i64>,
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Credential")
            .field("access_token", &asb_core::redact::REDACTED)
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

/// Queries the official usage endpoint with the CLI's cached login.
pub(crate) fn query(credentials: &Path, url: &str) -> Result<ClaudeNativeQuota, String> {
    let credential = read_native_credential(credentials)?;
    let now = chrono::Utc::now().timestamp_millis();
    let expired = credential.expires_at_ms.is_some_and(|at| at <= now);
    let headers = format!(
        "authorization: Bearer {}\r\nanthropic-beta: {OAUTH_BETA}\r\naccept: application/json",
        credential.access_token
    );
    let (status, body) = crate::probe::http_request("GET", url, &headers, &[])?;
    match status {
        200..=299 => {}
        401 | 403 => {
            return Err(
                "Claude 官方登录凭据已失效或没有订阅权限，请在官方登录入口或 Claude CLI 重新登录"
                    .into(),
            )
        }
        429 => return Err("Claude 官方用量接口请求过于频繁，请稍后重试".into()),
        code => return Err(format!("Claude 官方用量接口返回 HTTP {code}")),
    }
    let value: Value =
        serde_json::from_str(&body).map_err(|_| "Claude 官方用量接口响应不是有效 JSON")?;
    let (windows, extra_usage) = parse_usage(&value)?;
    Ok(ClaudeNativeQuota {
        windows,
        extra_usage,
        credential_expires_at_ms: credential.expires_at_ms,
        credential_expired: expired,
        checked_at_ms: now,
    })
}

fn read_native_credential(path: &Path) -> Result<Credential, String> {
    #[cfg(target_os = "macos")]
    if let Some(text) = keychain_credential() {
        return parse_credential(&text);
    }
    match std::fs::read_to_string(path) {
        Ok(text) => parse_credential(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err("未找到 Claude 官方登录凭据；请先在官方登录入口或 Claude CLI 登录".into())
        }
        Err(_) => Err("无法读取 Claude 官方登录凭据文件".into()),
    }
}

/// The Claude CLI stores its login in the login keychain on macOS; the
/// credentials file is only the fallback there.
#[cfg(target_os = "macos")]
fn keychain_credential() -> Option<String> {
    let output = std::process::Command::new("security")
        .args([
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_string()).filter(|value| !value.is_empty())
}

fn parse_credential(text: &str) -> Result<Credential, String> {
    let root: Value =
        serde_json::from_str(text).map_err(|_| "Claude 官方登录凭据格式无效，未改写该文件")?;
    let entry = root
        .get("claudeAiOauth")
        .or_else(|| root.get("claude.ai_oauth"))
        .and_then(Value::as_object)
        .ok_or("Claude 登录缓存中没有 claude.ai OAuth 凭据；请先完成官方登录")?;
    let access_token = entry
        .get("accessToken")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty() && !token.chars().any(char::is_control))
        .ok_or("Claude 登录缓存缺少有效的访问令牌；请重新登录")?
        .to_string();
    Ok(Credential {
        access_token,
        expires_at_ms: entry.get("expiresAt").and_then(parse_expires_at),
    })
}

/// Accepts Unix seconds, Unix milliseconds, or an RFC 3339 timestamp.
fn parse_expires_at(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => {
            let raw = number
                .as_i64()
                .or_else(|| number.as_f64().map(|n| n as i64))?;
            Some(if raw > 1_000_000_000_000 {
                raw
            } else {
                raw.checked_mul(1000)?
            })
        }
        Value::String(text) => chrono::DateTime::parse_from_rfc3339(text)
            .map(|date| date.timestamp_millis())
            .ok(),
        _ => None,
    }
}

fn parse_usage(
    body: &Value,
) -> Result<(Vec<ClaudeNativeQuotaWindow>, Option<ClaudeNativeExtraUsage>), String> {
    let object = body.as_object().ok_or("Claude 官方用量接口响应不是对象")?;
    let mut windows = Vec::new();
    for (id, label) in KNOWN_WINDOWS {
        if let Some(window) = object.get(id).and_then(|value| window(id, label, value)) {
            windows.push(window);
        }
    }
    for (id, value) in object {
        let known = id == "extra_usage" || KNOWN_WINDOWS.iter().any(|(known, _)| known == id);
        if !known {
            if let Some(window) = window(id, id, value) {
                windows.push(window);
            }
        }
    }
    let extra_usage = object
        .get("extra_usage")
        .and_then(Value::as_object)
        .map(|extra| ClaudeNativeExtraUsage {
            enabled: extra
                .get("is_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            monthly_limit: finite(extra.get("monthly_limit")),
            used_credits: finite(extra.get("used_credits")),
            utilization: finite(extra.get("utilization")),
            currency: extra
                .get("currency")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    Ok((windows, extra_usage))
}

fn window(id: &str, label: &str, value: &Value) -> Option<ClaudeNativeQuotaWindow> {
    let object = value.as_object()?;
    let used_percent = finite(object.get("utilization"))?;
    let resets_at_ms = object
        .get("resets_at")
        .and_then(Value::as_str)
        .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
        .map(|date| date.timestamp_millis());
    Some(ClaudeNativeQuotaWindow {
        id: id.to_string(),
        label: label.to_string(),
        used_percent,
        resets_at_ms,
    })
}

fn finite(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && *number >= 0.0)
}

#[cfg(test)]
mod tests;
