//! Local Claude managed-login sessions. A session never writes either client's native auth cache.

mod endpoints;
mod tokens;
use super::{contracts::validate_text, http, store, ClaudeAccount, ClaudeAuth};
use asb_core::claude_auth::ClaudeAuthProvider;
pub(crate) use endpoints::DeviceEndpoints;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeLoginRequest {
    pub provider: ClaudeAuthProvider,
    pub label: String,
    pub github_domain: Option<String>,
    pub target_account_id: Option<String>,
    pub make_default: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeLoginView {
    pub session_id: String,
    pub phase: &'static str,
    pub user_code: String,
    pub verification_url: String,
    pub interval_seconds: u64,
    pub expires_at_ms: i64,
    pub account_id: Option<String>,
}

#[derive(Clone)]
pub(super) struct Session {
    provider: ClaudeAuthProvider,
    label: String,
    domain: String,
    target_account_id: Option<String>,
    target_revision: Option<String>,
    default_at_start: Option<String>,
    make_default: bool,
    endpoints: DeviceEndpoints,
    device_code: String,
    user_code: String,
    verification_url: String,
    interval_seconds: u64,
    next_poll_ms: i64,
    expires_at_ms: i64,
    ready: Option<ClaudeAccount>,
    tokens: Option<Value>,
    token_received_at_ms: Option<i64>,
    polling: bool,
}

impl Session {
    fn view(&self, id: &str, account_id: Option<String>) -> ClaudeLoginView {
        ClaudeLoginView {
            session_id: id.into(),
            phase: if account_id.is_some() {
                "completed"
            } else {
                "pending"
            },
            user_code: self.user_code.clone(),
            verification_url: self.verification_url.clone(),
            interval_seconds: self.interval_seconds,
            expires_at_ms: self.expires_at_ms,
            account_id,
        }
    }
}

impl ClaudeAuth {
    pub(crate) fn start_login(
        &self,
        request: ClaudeLoginRequest,
    ) -> Result<ClaudeLoginView, String> {
        validate_text(&request.label, "账号名称", 256)?;
        if request.provider != ClaudeAuthProvider::GithubCopilot && request.github_domain.is_some()
        {
            return Err("只有 Copilot 登录可以选择 GitHub 域名".into());
        }
        let endpoints = DeviceEndpoints::discover(
            request.provider,
            request.github_domain.as_deref().unwrap_or("github.com"),
        )?;
        self.start_login_at(request, endpoints, chrono::Utc::now().timestamp_millis())
    }

    fn start_login_at(
        &self,
        request: ClaudeLoginRequest,
        endpoints: DeviceEndpoints,
        now: i64,
    ) -> Result<ClaudeLoginView, String> {
        let mut runtime = self.runtime.lock().map_err(|_| "Claude 登录锁不可用")?;
        runtime.sessions.retain(|_, session| {
            session.expires_at_ms > now || session.tokens.is_some() || session.ready.is_some()
        });
        if runtime.sessions.len() >= 16 {
            return Err("Claude 等待中的登录过多，请先取消或完成已有登录".into());
        }
        let (file, _) = store::load(&self.root)?;
        let target_revision = request
            .target_account_id
            .as_ref()
            .map(|id| {
                let account = file
                    .accounts
                    .iter()
                    .find(|a| &a.id == id && a.provider == request.provider)
                    .ok_or("Claude 要重新登录的账号不存在或认证服务不匹配")?;
                revision(account)
            })
            .transpose()?;
        let default_at_start = file.defaults.get(&request.provider).cloned();
        let value = start_request(request.provider, &endpoints)?;
        let mut session = session(request, endpoints, value, now, target_revision)?;
        session.default_at_start = default_at_start;
        let id = uuid::Uuid::new_v4().to_string();
        let view = session.view(&id, None);
        runtime.sessions.insert(id, session);
        Ok(view)
    }

    pub(crate) fn poll_login(&self, id: &str) -> Result<ClaudeLoginView, String> {
        self.poll_login_at(id, chrono::Utc::now().timestamp_millis())
    }

    fn poll_login_at(&self, id: &str, now: i64) -> Result<ClaudeLoginView, String> {
        let mut session = {
            let mut runtime = self.runtime.lock().map_err(|_| "Claude 登录锁不可用")?;
            let session = runtime
                .sessions
                .get_mut(id)
                .ok_or("Claude 登录已取消、结束或不存在")?;
            if now >= session.expires_at_ms && session.tokens.is_none() && session.ready.is_none() {
                runtime.sessions.remove(id);
                return Err("Claude 登录已过期，请重新开始".into());
            }
            if session.polling || (session.ready.is_none() && now < session.next_poll_ms) {
                return Ok(session.view(id, None));
            }
            session.polling = true;
            session.next_poll_ms = now + session.interval_seconds as i64 * 1000;
            session.clone()
        };
        let result = poll_network(&mut session, now);
        let mut runtime = self.runtime.lock().map_err(|_| "Claude 登录锁不可用")?;
        let current = runtime
            .sessions
            .get_mut(id)
            .ok_or("Claude 登录已取消，不保存返回的凭据")?;
        session.polling = false;
        *current = session;
        result?;
        let Some(account) = current.ready.as_ref() else {
            return Ok(current.view(id, None));
        };
        commit_login(&self.root, current, account)?;
        let account_id = account.id.clone();
        let view = current.view(id, Some(account_id.clone()));
        runtime.copilot.remove(&account_id);
        runtime.sessions.remove(id);
        Ok(view)
    }

    pub(crate) fn cancel_login(&self, id: &str) -> Result<(), String> {
        self.runtime
            .lock()
            .map_err(|_| "Claude 登录锁不可用")?
            .sessions
            .remove(id);
        Ok(())
    }
}

fn revision(account: &ClaudeAccount) -> Result<String, String> {
    serde_json::to_string(account)
        .map(|text| asb_switch::sha256_hex(&text))
        .map_err(|_| "Claude 账号修订无法计算".into())
}

fn start_request(
    provider: ClaudeAuthProvider,
    endpoints: &DeviceEndpoints,
) -> Result<Value, String> {
    let (content_type, body) = if provider == ClaudeAuthProvider::CodexOauth {
        (
            "application/json",
            json!({"client_id": endpoints.client_id})
                .to_string()
                .into_bytes(),
        )
    } else {
        (
            "application/x-www-form-urlencoded",
            http::form_body(&[
                ("client_id", &endpoints.client_id),
                ("scope", &endpoints.scope),
            ]),
        )
    };
    http::json_request(
        reqwest::Method::POST,
        &endpoints.start,
        &[
            ("content-type", content_type),
            ("accept", "application/json"),
        ],
        body,
    )
}

fn session(
    request: ClaudeLoginRequest,
    endpoints: DeviceEndpoints,
    value: Value,
    now: i64,
    target_revision: Option<String>,
) -> Result<Session, String> {
    let text = |key: &str| -> Result<String, String> {
        let text = value[key]
            .as_str()
            .ok_or_else(|| format!("Claude 登录响应缺少 {key}"))?;
        validate_text(text, key, 4096)?;
        Ok(text.into())
    };
    let chatgpt = request.provider == ClaudeAuthProvider::CodexOauth;
    let device_code = text(if chatgpt {
        "device_auth_id"
    } else {
        "device_code"
    })?;
    let user_code = text("user_code")?;
    let verification_url = value
        .get("verification_uri_complete")
        .or_else(|| value.get("verification_uri"))
        .and_then(Value::as_str)
        .unwrap_or(&endpoints.verification)
        .to_string();
    validate_verification_url(&verification_url)?;
    let interval_seconds = value["interval"]
        .as_u64()
        .or_else(|| value["interval"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(5)
        .clamp(1, 60);
    let lifetime = value["expires_in"].as_i64().unwrap_or(900);
    if !(1..=86400).contains(&lifetime) {
        return Err("Claude 登录设备码有效期无效".into());
    }
    Ok(Session {
        provider: request.provider,
        label: request.label,
        domain: request.github_domain.unwrap_or("github.com".into()),
        target_account_id: request.target_account_id,
        target_revision,
        default_at_start: None,
        make_default: request.make_default,
        endpoints,
        device_code,
        user_code,
        verification_url,
        interval_seconds,
        next_poll_ms: now + interval_seconds as i64 * 1000,
        expires_at_ms: now + lifetime * 1000,
        ready: None,
        tokens: None,
        token_received_at_ms: None,
        polling: false,
    })
}

fn validate_verification_url(raw: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(raw).map_err(|_| "Claude 登录验证地址无效")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("Claude 登录验证地址必须是无凭据的 HTTPS 地址".into());
    }
    Ok(())
}

fn poll_request(session: &mut Session) -> Result<Option<Value>, String> {
    let chatgpt = session.provider == ClaudeAuthProvider::CodexOauth;
    let (kind, body) = if chatgpt {
        (
            "application/json",
            json!({"device_auth_id":session.device_code,"user_code":session.user_code})
                .to_string()
                .into_bytes(),
        )
    } else {
        (
            "application/x-www-form-urlencoded",
            http::form_body(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", &session.endpoints.client_id),
                ("device_code", &session.device_code),
            ]),
        )
    };
    let (status, value) = http::json_response(
        reqwest::Method::POST,
        &session.endpoints.poll,
        &[("content-type", kind), ("accept", "application/json")],
        body,
    )?;
    if chatgpt && matches!(status, 403 | 404) {
        return Ok(None);
    }
    match value.get("error").and_then(Value::as_str) {
        Some("authorization_pending") => return Ok(None),
        Some("slow_down") => {
            session.interval_seconds = (session.interval_seconds + 5).min(60);
            session.next_poll_ms += 5000;
            return Ok(None);
        }
        Some("expired_token" | "access_denied" | "authorization_declined") => {
            session.expires_at_ms = 0;
            return Err("Claude 登录已被拒绝或设备码已过期，请重新开始".into());
        }
        Some(_) => return Err("Claude 登录授权失败，请重新开始".into()),
        None => {}
    }
    if !(200..300).contains(&status) {
        return Err(http::status_error(status));
    }
    if chatgpt {
        tokens::exchange(&session.endpoints, value).map(Some)
    } else {
        Ok(Some(value))
    }
}

fn poll_network(session: &mut Session, now: i64) -> Result<(), String> {
    if session.ready.is_some() {
        return Ok(());
    }
    if session.tokens.is_none() {
        session.tokens = poll_request(session)?;
        session.token_received_at_ms = session.tokens.as_ref().map(|_| now);
    }
    if let Some(tokens) = &session.tokens {
        session.ready = Some(tokens::account(
            session,
            tokens.clone(),
            session.token_received_at_ms.unwrap_or(now),
        )?);
    }
    Ok(())
}

fn commit_login(
    root: &std::path::Path,
    session: &Session,
    account: &ClaudeAccount,
) -> Result<(), String> {
    let (mut file, hash) = store::load(root)?;
    let current = file.accounts.iter().find(|saved| saved.id == account.id);
    if session.target_revision.as_ref().is_some_and(|expected| {
        current.and_then(|saved| revision(saved).ok()).as_ref() != Some(expected)
    }) {
        return Err("Claude 重新登录期间目标账号已改变；请取消后重新开始，未覆盖已有账号".into());
    }
    if current.is_some_and(|saved| saved.provider != account.provider) {
        return Err("Claude 登录账号 ID 与其他认证服务冲突".into());
    }
    // A newer explicit account choice wins over a pending login's initial default request.
    if !file.defaults.contains_key(&account.provider)
        || (session.make_default
            && file.defaults.get(&account.provider) == session.default_at_start.as_ref())
    {
        file.defaults.insert(account.provider, account.id.clone());
    }
    file.accounts.retain(|saved| saved.id != account.id);
    file.accounts.push(account.clone());
    store::save(root, &file, &hash).map(|_| ())
}
