//! OAuth refresh for Claude upstream accounts, not the Codex client's auth.json.

use super::{
    contracts::validate_text,
    http::{form_body, json_request},
    ClaudeAccount,
};
use asb_core::claude_auth::ClaudeAuthProvider;

pub(super) const CHATGPT_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub(super) const XAI_DISCOVERY_URL: &str = "https://auth.x.ai/.well-known/openid-configuration";
pub(super) const CHATGPT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub(super) const XAI_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";

pub(super) const XAI_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";

pub(super) fn refresh(
    account: &ClaudeAccount,
    token_url: &str,
    now: i64,
) -> Result<ClaudeAccount, String> {
    let refresh = account
        .refresh_token
        .as_deref()
        .ok_or("Claude 托管账号已过期且没有刷新凭据，请重新登录")?;
    let client_id = match account.provider {
        ClaudeAuthProvider::CodexOauth => CHATGPT_CLIENT_ID,
        ClaudeAuthProvider::XaiOauth => XAI_CLIENT_ID,
        ClaudeAuthProvider::GithubCopilot => return Err("Copilot 必须通过 GitHub 交换令牌".into()),
    };
    let mut fields = vec![
        ("grant_type", "refresh_token"),
        ("client_id", client_id),
        ("refresh_token", refresh),
    ];
    if account.provider == ClaudeAuthProvider::XaiOauth {
        fields.push(("scope", XAI_SCOPE));
    }
    let response = json_request(
        reqwest::Method::POST,
        token_url,
        &[
            ("content-type", "application/x-www-form-urlencoded"),
            ("accept", "application/json"),
        ],
        form_body(&fields),
    )?;
    if response.get("error").is_some() {
        return Err("Claude OAuth 刷新被拒绝，请重新登录；原凭据未修改".into());
    }
    let token = response["access_token"]
        .as_str()
        .ok_or("Claude OAuth 响应缺少访问令牌")?;
    validate_text(token, "OAuth 访问令牌", 32768)?;
    let expires_at_ms = response["expires_in"]
        .as_i64()
        .filter(|seconds| *seconds > 0)
        .and_then(|seconds| seconds.checked_mul(1000))
        .and_then(|ms| now.checked_add(ms))
        .ok_or("Claude OAuth 响应到期时间无效")?;
    let mut next = account.clone();
    next.access_token = token.into();
    next.expires_at_ms = Some(expires_at_ms);
    if let Some(refresh) = response.get("refresh_token") {
        let refresh = refresh.as_str().ok_or("Claude OAuth 刷新凭据格式无效")?;
        validate_text(refresh, "OAuth 刷新凭据", 32768)?;
        next.refresh_token = Some(refresh.into());
    }
    next.validate()?;
    Ok(next)
}

pub(super) fn xai_token_endpoint(discovery_url: &str) -> Result<String, String> {
    let value = json_request(
        reqwest::Method::GET,
        discovery_url,
        &[("accept", "application/json")],
        Vec::new(),
    )?;
    if value["issuer"] != "https://auth.x.ai" {
        return Err("xAI 认证服务 issuer 不匹配".into());
    }
    let raw = value["token_endpoint"]
        .as_str()
        .ok_or("xAI 认证发现缺少 token_endpoint")?;
    validate_xai_endpoint(raw)?;
    Ok(raw.into())
}

pub(super) fn validate_xai_endpoint(raw: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(raw).map_err(|_| "xAI 认证端点无效")?;
    if url.scheme() != "https"
        || url.host_str() != Some("auth.x.ai")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("xAI 认证发现返回了不可信端点".into());
    }
    Ok(())
}
