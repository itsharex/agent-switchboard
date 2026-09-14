use super::contracts::Tokens;
use crate::official_login::{codex::CODEX_CLIENT_ID, percent_encode};
use serde::Deserialize;

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
}
pub(super) fn refresh(tokens: &Tokens) -> Result<Tokens, String> {
    refresh_at(tokens, "https://auth.openai.com/oauth/token")
}
pub(super) fn refresh_at(tokens: &Tokens, url: &str) -> Result<Tokens, String> {
    let form = [
        ("grant_type", "refresh_token"),
        ("client_id", CODEX_CLIENT_ID),
        ("refresh_token", &tokens.refresh_token),
    ]
    .map(|(key, value)| format!("{key}={}", percent_encode(value)))
    .join("&");
    let (status, body) = crate::probe::http_request(
        "POST",
        url,
        "Content-Type: application/x-www-form-urlencoded\r\nAccept: application/json\r\n",
        form.as_bytes(),
    )
    .map_err(|_| "Codex 登录刷新网络失败，可稍后重试")?;
    if matches!(status, 400 | 401 | 403) {
        return Err(format!("Codex 登录已失效，请重新认证（HTTP {status}）"));
    }
    if status != 200 {
        return Err(format!(
            "Codex 登录刷新暂时失败（HTTP {status}），请稍后重试"
        ));
    }
    let response: TokenResponse =
        serde_json::from_str(&body).map_err(|_| "Codex 登录刷新响应无效")?;
    Ok(Tokens {
        access_token: response.access_token,
        refresh_token: response
            .refresh_token
            .unwrap_or_else(|| tokens.refresh_token.clone()),
        id_token: response.id_token.unwrap_or_else(|| tokens.id_token.clone()),
    })
}
