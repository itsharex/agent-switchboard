use super::contracts::{Identity, Tokens};
use crate::official_login::credentials::jwt_payload;
use serde_json::Value;

pub(super) fn identity(tokens: &Tokens) -> Result<Identity, String> {
    if [
        &tokens.id_token,
        &tokens.access_token,
        &tokens.refresh_token,
    ]
    .iter()
    .any(|value| value.trim().is_empty() || value.chars().any(char::is_control))
    {
        return Err("Codex 登录令牌不完整或无效，请重新认证".into());
    }
    let claims =
        jwt_payload(&tokens.id_token).ok_or("Codex 登录缺少可识别的 id_token，请重新认证")?;
    let auth = claims
        .get("https://api.openai.com/auth")
        .or_else(|| claims.get("auth"));
    let account_id = auth
        .and_then(|a| a.get("chatgpt_account_id").or_else(|| a.get("account_id")))
        .or_else(|| claims.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty() && !v.chars().any(char::is_control))
        .ok_or("Codex 登录缺少 ChatGPT 账号标识，请重新认证")?;
    let subject = auth
        .and_then(|a| a.get("chatgpt_user_id").or_else(|| a.get("user_id")))
        .or_else(|| claims.get("sub"))
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or("Codex 登录缺少用户身份，请重新认证")?;
    Ok(Identity {
        subject: subject.into(),
        account_id: account_id.into(),
        email: claims
            .get("email")
            .and_then(Value::as_str)
            .map(str::to_string),
        plan: auth
            .and_then(|a| a.get("chatgpt_plan_type"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

pub(super) fn expires_at(tokens: &Tokens, now: i64) -> i64 {
    jwt_payload(&tokens.access_token)
        .and_then(|p| p.get("exp").and_then(Value::as_i64))
        .and_then(|exp| exp.checked_mul(1000))
        .unwrap_or(now + 3_600_000)
}

pub(super) fn native(text: &str) -> Result<(Tokens, i64), String> {
    let value: Value = serde_json::from_str(text).map_err(|_| "Codex auth.json 不是有效 JSON")?;
    if value
        .get("OPENAI_API_KEY")
        .and_then(Value::as_str)
        .is_some_and(|v| !v.trim().is_empty())
    {
        return Err("当前 Codex 使用 API key，不是可导入的 ChatGPT 登录".into());
    }
    let get = |key: &str| {
        value
            .get("tokens")
            .and_then(|v| v.get(key))
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .ok_or_else(|| "Codex 登录令牌不完整，请重新认证".to_string())
    };
    let tokens = Tokens {
        id_token: get("id_token")?,
        access_token: get("access_token")?,
        refresh_token: get("refresh_token")?,
    };
    identity(&tokens)?;
    let updated = value
        .get("last_refresh")
        .and_then(Value::as_str)
        .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
        .map(|v| v.timestamp_millis())
        .unwrap_or(0);
    Ok((tokens, updated))
}
