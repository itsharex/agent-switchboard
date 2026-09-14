//! Tokens are decoded only after the authorization service successfully exchanges a code.

use super::super::{contracts::validate_text, http::json_request, ClaudeAccount};
use super::{DeviceEndpoints, Session};
use asb_core::claude_auth::ClaudeAuthProvider;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};

pub(super) fn account(session: &Session, tokens: Value, now: i64) -> Result<ClaudeAccount, String> {
    let token = tokens["access_token"]
        .as_str()
        .ok_or("Claude 登录响应缺少访问凭据")?;
    validate_text(token, "登录凭据", 32768)?;
    if session.provider == ClaudeAuthProvider::GithubCopilot {
        return github_account(session, token);
    }
    let claims = tokens
        .get("id_token")
        .and_then(Value::as_str)
        .and_then(jwt)
        .or_else(|| jwt(token))
        .ok_or("Claude OAuth 响应缺少账号身份")?;
    let access_claims = jwt(token).unwrap_or(json!({}));
    let subject = claims["sub"].as_str().ok_or("Claude OAuth 身份缺少 sub")?;
    validate_text(subject, "OAuth 身份", 256)?;
    let workspace = claims
        .pointer("/https:~1~1api.openai.com~1auth/chatgpt_account_id")
        .or_else(|| access_claims.pointer("/https:~1~1api.openai.com~1auth/chatgpt_account_id"))
        .or_else(|| claims.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let expiry = tokens["expires_in"]
        .as_i64()
        .filter(|seconds| *seconds > 0)
        .and_then(|seconds| seconds.checked_mul(1000))
        .and_then(|ms| now.checked_add(ms))
        .or_else(|| {
            access_claims["exp"]
                .as_i64()
                .and_then(|seconds| seconds.checked_mul(1000))
        })
        .filter(|expiry| *expiry > now)
        .ok_or("Claude OAuth 响应到期时间无效")?;
    let identity = format!(
        "{:?}:{subject}:{}",
        session.provider,
        workspace.as_deref().unwrap_or("")
    );
    let account = ClaudeAccount {
        id: session
            .target_account_id
            .clone()
            .unwrap_or_else(|| asb_switch::sha256_hex(&identity)[..32].to_string()),
        label: session.label.clone(),
        provider: session.provider,
        access_token: token.into(),
        refresh_token: tokens
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::to_string),
        expires_at_ms: Some(expiry),
        upstream_account_id: workspace,
        github_domain: None,
    };
    account.validate()?;
    Ok(account)
}

fn github_account(session: &Session, token: &str) -> Result<ClaudeAccount, String> {
    let authorization = format!("Bearer {token}");
    let user = json_request(
        reqwest::Method::GET,
        &session.endpoints.user,
        &[
            ("authorization", &authorization),
            ("accept", "application/json"),
        ],
        Vec::new(),
    )?;
    let id = user["id"].as_u64().ok_or("GitHub 登录响应缺少账号 ID")?;
    let id = if session.domain == "github.com" {
        id.to_string()
    } else {
        format!("{}:{id}", session.domain)
    };
    let account = ClaudeAccount {
        id: session.target_account_id.clone().unwrap_or(id),
        label: session.label.clone(),
        provider: session.provider,
        access_token: token.into(),
        refresh_token: None,
        expires_at_ms: None,
        upstream_account_id: None,
        github_domain: Some(session.domain.clone()),
    };
    account.validate()?;
    Ok(account)
}

fn jwt(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub(super) fn exchange(endpoints: &DeviceEndpoints, value: Value) -> Result<Value, String> {
    let code = value["authorization_code"]
        .as_str()
        .ok_or("Claude ChatGPT 登录响应缺少授权码")?;
    let verifier = value["code_verifier"]
        .as_str()
        .ok_or("Claude ChatGPT 登录响应缺少 PKCE 校验码")?;
    json_request(
        reqwest::Method::POST,
        &endpoints.token,
        &[("content-type", "application/x-www-form-urlencoded")],
        super::super::http::form_body(&[
            ("grant_type", "authorization_code"),
            ("client_id", &endpoints.client_id),
            (
                "redirect_uri",
                "https://auth.openai.com/deviceauth/callback",
            ),
            ("code", code),
            ("code_verifier", verifier),
        ]),
    )
}
