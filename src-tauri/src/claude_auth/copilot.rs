//! Claude's Copilot token exchange and enterprise endpoint resolution.

use super::{contracts::validate_text, http::json_request, ClaudeAccount, ResolvedAccount};
use serde_json::Value;

pub(super) fn validate_domain(domain: &str) -> Result<(), String> {
    if domain.len() > 253
        || !domain.contains('.')
        || domain != domain.to_ascii_lowercase()
        || domain.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err("GitHub 域名必须是小写主机名，不能包含协议、端口或路径".into());
    }
    Ok(())
}

pub(super) fn api_base(account: &ClaudeAccount) -> String {
    match account.github_domain.as_deref().unwrap_or("github.com") {
        "github.com" => "https://api.github.com".into(),
        domain => format!("https://{domain}/api/v3"),
    }
}

pub(super) fn exchange(
    account: &ClaudeAccount,
    base: &str,
    now: i64,
) -> Result<ResolvedAccount, String> {
    let authorization = format!("token {}", account.access_token);
    let headers = [
        ("authorization", authorization.as_str()),
        ("accept", "application/json"),
        ("editor-version", "vscode/1.110.1"),
        ("editor-plugin-version", "copilot-chat/0.38.2"),
        ("x-github-api-version", "2022-11-28"),
    ];
    let response = json_request(
        reqwest::Method::GET,
        &format!("{base}/copilot_internal/v2/token"),
        &headers,
        Vec::new(),
    )?;
    let token = response["token"]
        .as_str()
        .ok_or("Copilot 认证响应缺少访问令牌")?;
    validate_text(token, "Copilot 访问令牌", 32768)?;
    let expires_at_ms = response["expires_at"]
        .as_i64()
        .and_then(|seconds| seconds.checked_mul(1000))
        .filter(|expiry| *expiry > now)
        .ok_or("Copilot 返回无效或已过期的令牌")?;
    let details = json_request(
        reqwest::Method::GET,
        &format!("{base}/copilot_internal/user"),
        &headers,
        Vec::new(),
    )?;
    let endpoint = endpoint(account, &details)?;
    Ok(ResolvedAccount {
        provider: account.provider,
        account_id: account.id.clone(),
        access_token: token.into(),
        expires_at_ms,
        endpoint,
        upstream_account_id: None,
    })
}

fn endpoint(account: &ClaudeAccount, details: &Value) -> Result<String, String> {
    if let Some(endpoint) = details.get("endpoints").and_then(|value| value.get("api")) {
        let raw = endpoint.as_str().ok_or("Copilot 账号 API 端点格式无效")?;
        let url = reqwest::Url::parse(raw).map_err(|_| "Copilot 账号 API 端点无效")?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("Copilot 账号 API 端点必须是 HTTPS 地址".into());
        }
        return Ok(raw.trim_end_matches('/').into());
    }
    Ok(
        match account.github_domain.as_deref().unwrap_or("github.com") {
            "github.com" => "https://api.githubcopilot.com".into(),
            domain => format!("https://copilot-api.{domain}"),
        },
    )
}
