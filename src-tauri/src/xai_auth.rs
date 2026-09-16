//! xAI OAuth (SuperGrok) as a Codex native-Responses upstream (A07).
//!
//! xAI uses the OAuth 2.0 Device Authorization Grant against the
//! `auth.x.ai` issuer, resolved from its OpenID Connect discovery document.
//! This module owns the xAI account store, the device flow, token refresh,
//! and per-request credential resolution. Access tokens never leave this
//! module towards IPC: commands and the gateway consume
//! [`valid_token_for_profile`], which resolves the bound (or default)
//! account and refreshes when the token is expiring.

use std::{
    fs,
    path::Path,
    sync::{Mutex, MutexGuard, OnceLock},
};

use asb_switch::{io::FsIo, sha256_hex, SwitchIo};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::official_login::credentials::jwt_payload;
use crate::probe::http_request;

const DISCOVERY_URL: &str = "https://auth.x.ai/.well-known/openid-configuration";
const CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";
const USER_AGENT: &str = "agent-switchboard-xai-oauth";
const REFRESH_BUFFER_MS: i64 = 60_000;
const DEFAULT_TOKEN_LIFETIME_MS: i64 = 3_600_000;

const UNRECOGNIZED_RESPONSE: &str = "xAI 登录服务响应无法识别";
const RELOGIN_REQUIRED: &str = "xAI 授权已失效，请重新登录 xAI 账号";

fn lock() -> Result<MutexGuard<'static, ()>, String> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "xAI 账号锁不可用，请重启应用".into())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct XaiTokens {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct XaiAccount {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) tokens: XaiTokens,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct XaiFile {
    version: u8,
    #[serde(default)]
    default_id: Option<String>,
    #[serde(default)]
    accounts: Vec<XaiAccount>,
    /// Codex profile id → bound xAI account id; unbound profiles fall back
    /// to the default account.
    #[serde(default)]
    bindings: std::collections::BTreeMap<String, String>,
}

impl Default for XaiTokens {
    fn default() -> Self {
        Self {
            access_token: String::new(),
            refresh_token: String::new(),
            expires_at_ms: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct XaiResolution {
    pub(crate) account_id: String,
    pub(crate) access_token: String,
}

fn store_path(root: &Path) -> std::path::PathBuf {
    root.join("xai/accounts.json")
}

fn load_file(root: &Path) -> Result<(XaiFile, String), String> {
    let raw = match fs::read_to_string(store_path(root)) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((
                XaiFile {
                    version: 1,
                    ..Default::default()
                },
                sha256_hex(""),
            ))
        }
        Err(_) => return Err("无法读取 xAI 账号库".into()),
    };
    let file: XaiFile =
        serde_json::from_str(&raw).map_err(|_| "xAI 账号库格式无效，请从备份恢复".to_string())?;
    Ok((file, sha256_hex(&raw)))
}

fn save_file(root: &Path, file: &XaiFile, expected: &str) -> Result<String, String> {
    let text = serde_json::to_string_pretty(file).map_err(|_| "xAI 账号库无法序列化")?;
    if load_unlocked(root)? != expected {
        return Err("xAI 账号库已变化，请刷新后重试".into());
    }
    let path = store_path(root);
    let parent = path.parent().ok_or("xAI 账号库路径无效")?;
    fs::create_dir_all(parent).map_err(|_| "无法创建 xAI 账号目录")?;
    let temporary = parent.join(format!("accounts.{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        FsIo.write_new_file(&temporary, &text)
            .map_err(|_| "无法写入 xAI 账号临时文件")?;
        fs::rename(&temporary, &path).map_err(|_| "无法原子替换 xAI 账号库")?;
        FsIo.sync_dir(parent)
            .map_err(|_| "xAI 账号库已保存，但目录同步失败")?;
        Ok::<_, String>(sha256_hex(&text))
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn load_unlocked(root: &Path) -> Result<String, String> {
    match fs::read_to_string(store_path(root)) {
        Ok(raw) => Ok(sha256_hex(&raw)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(sha256_hex("")),
        Err(_) => Err("无法读取 xAI 账号库".into()),
    }
}

/// Resolves the OpenID Connect endpoints so protocol changes do not require
/// duplicating constants; tests point the discovery document at a loopback
/// fake.
pub(crate) struct XaiEndpoints {
    pub(crate) device_authorization_endpoint: String,
    pub(crate) token_endpoint: String,
}

fn resolve_endpoints(discovery_url: &str) -> Result<XaiEndpoints, String> {
    let (status, body) = http_request(
        "GET",
        discovery_url,
        &format!("Accept: application/json\r\nUser-Agent: {USER_AGENT}\r\n"),
        b"",
    )
    .map_err(|error| format!("无法解析 xAI 登录端点：{error}"))?;
    if status != 200 {
        return Err(format!("无法解析 xAI 登录端点：HTTP {status}"));
    }
    #[derive(Deserialize)]
    struct Discovery {
        token_endpoint: Option<String>,
        device_authorization_endpoint: Option<String>,
    }
    let parsed: Discovery =
        serde_json::from_str(&body).map_err(|_| UNRECOGNIZED_RESPONSE.to_string())?;
    // Loopback endpoints exist only for isolated flow tests.
    let trusted = |value: &Option<String>| {
        value.as_deref().is_some_and(|value| {
            value.starts_with("https://") || value.starts_with("http://127.0.0.1:")
        })
    };
    if !trusted(&parsed.token_endpoint) || !trusted(&parsed.device_authorization_endpoint) {
        return Err(UNRECOGNIZED_RESPONSE.to_string());
    }
    let token = parsed.token_endpoint.unwrap();
    let device = parsed.device_authorization_endpoint.unwrap();
    Ok(XaiEndpoints {
        device_authorization_endpoint: device,
        token_endpoint: token,
    })
}

fn claims_label(claims: &Value) -> String {
    for key in ["preferred_username", "email", "name", "sub"] {
        if let Some(value) = claims.get(key).and_then(Value::as_str) {
            if !value.trim().is_empty() {
                return value.to_string();
            }
        }
    }
    "xAI 账号".into()
}

fn claims_sub(claims: &Value) -> Option<String> {
    claims
        .get("sub")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// One started device login. The device code is persisted so polling, crash
/// recovery, and cancellation share one source of truth.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XaiLoginSession {
    pub(crate) user_code: String,
    pub(crate) verification_uri: String,
    pub(crate) expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PendingLogin {
    device_code: String,
    token_endpoint: String,
    expires_at_ms: i64,
    interval_ms: i64,
}

fn pending_path(root: &Path) -> std::path::PathBuf {
    root.join("xai/pending-login.json")
}

/// Starts a device authorization against the issuer's discovery endpoints.
pub(crate) fn start_login(root: &Path) -> Result<XaiLoginSession, String> {
    start_login_at(root, DISCOVERY_URL, chrono::Utc::now().timestamp_millis())
}

pub(crate) fn start_login_at(
    root: &Path,
    discovery_url: &str,
    now_ms: i64,
) -> Result<XaiLoginSession, String> {
    let _guard = lock()?;
    let endpoints = resolve_endpoints(discovery_url)?;
    let form = [("client_id", CLIENT_ID), ("scope", SCOPE)]
        .map(|(key, value)| format!("{key}={}", crate::official_login::percent_encode(value)))
        .join("&");
    let (status, body) = http_request(
        "POST",
        &endpoints.device_authorization_endpoint,
        &format!(
            "Content-Type: application/x-www-form-urlencoded\r\nAccept: application/json\r\nUser-Agent: {USER_AGENT}\r\n"
        ),
        form.as_bytes(),
    )
    .map_err(|error| format!("xAI 登录服务不可达：{error}"))?;
    if status != 200 {
        return Err(format!("xAI 登录服务拒绝了设备授权请求：HTTP {status}"));
    }
    #[derive(Deserialize)]
    struct DeviceGrant {
        device_code: Option<String>,
        user_code: Option<String>,
        verification_uri: Option<String>,
        #[serde(default)]
        expires_in: Option<u64>,
        #[serde(default)]
        interval: Option<u64>,
    }
    let parsed: DeviceGrant =
        serde_json::from_str(&body).map_err(|_| UNRECOGNIZED_RESPONSE.to_string())?;
    let device_code = parsed
        .device_code
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UNRECOGNIZED_RESPONSE.to_string())?;
    let user_code = parsed
        .user_code
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UNRECOGNIZED_RESPONSE.to_string())?;
    let verification_uri = parsed
        .verification_uri
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UNRECOGNIZED_RESPONSE.to_string())?;
    let pending = PendingLogin {
        device_code,
        token_endpoint: endpoints.token_endpoint,
        expires_at_ms: now_ms + (parsed.expires_in.unwrap_or(900) as i64) * 1000,
        interval_ms: (parsed.interval.unwrap_or(5).min(60) as i64) * 1000,
    };
    let session = XaiLoginSession {
        user_code: user_code.clone(),
        verification_uri,
        expires_at_ms: pending.expires_at_ms,
    };
    let text = serde_json::to_string(&pending).map_err(|_| "登录会话无法序列化")?;
    let path = pending_path(root);
    fs::create_dir_all(path.parent().ok_or("登录会话路径无效")?)
        .map_err(|_| "无法创建登录会话目录")?;
    fs::write(&path, text).map_err(|_| "无法保存登录会话")?;
    Ok(session)
}

#[derive(Debug)]
pub(crate) enum XaiPollOutcome {
    Pending,
    Completed,
}

/// Runs one poll step. Transport errors and `authorization_pending` stay
/// pending so a seconds-interval UI poll survives hiccups.
pub(crate) fn poll_login(root: &Path) -> Result<XaiPollOutcome, String> {
    poll_login_at(root, chrono::Utc::now().timestamp_millis())
}

pub(crate) fn poll_login_at(root: &Path, now_ms: i64) -> Result<XaiPollOutcome, String> {
    let _guard = lock()?;
    let raw = match fs::read_to_string(pending_path(root)) {
        Ok(value) => value,
        Err(_) => return Err("没有进行中的 xAI 登录".into()),
    };
    let pending: PendingLogin =
        serde_json::from_str(&raw).map_err(|_| "登录会话已损坏，请取消后重试".to_string())?;
    if now_ms >= pending.expires_at_ms {
        let _ = fs::remove_file(pending_path(root));
        return Err("xAI 设备码已过期，请重新开始登录".into());
    }
    let form = [
        (
            "grant_type",
            "urn:ietf:params:oauth:grant-type:device_code".to_string(),
        ),
        ("device_code", pending.device_code.clone()),
        ("client_id", CLIENT_ID.to_string()),
    ]
    .map(|(key, value)| format!("{key}={}", crate::official_login::percent_encode(&value)))
    .join("&");
    let (status, body) = match http_request(
        "POST",
        &pending.token_endpoint,
        &format!(
            "Content-Type: application/x-www-form-urlencoded\r\nAccept: application/json\r\nUser-Agent: {USER_AGENT}\r\n"
        ),
        form.as_bytes(),
    ) {
        Ok(outcome) => outcome,
        Err(_) => return Ok(XaiPollOutcome::Pending),
    };
    match status {
        200 => {}
        // RFC 8628: the token endpoint answers 400 with an `error` code while
        // the user has not approved yet.
        400 => {
            let pending_code = body.contains("authorization_pending") || body.contains("slow_down");
            if pending_code {
                return Ok(XaiPollOutcome::Pending);
            }
            let _ = fs::remove_file(pending_path(root));
            if body.contains("expired_token") {
                return Err("xAI 设备码已过期，请重新开始登录".into());
            }
            if body.contains("access_denied") {
                return Err("用户拒绝了 xAI 授权".into());
            }
            return Err("xAI 登录被拒绝".into());
        }
        status if status >= 500 => return Ok(XaiPollOutcome::Pending),
        _ => return Err(UNRECOGNIZED_RESPONSE.to_string()),
    }
    #[derive(Deserialize)]
    struct TokenGrant {
        access_token: Option<String>,
        #[serde(default)]
        refresh_token: Option<String>,
        #[serde(default)]
        id_token: Option<String>,
        #[serde(default)]
        expires_in: Option<i64>,
    }
    let parsed: TokenGrant =
        serde_json::from_str(&body).map_err(|_| UNRECOGNIZED_RESPONSE.to_string())?;
    let access_token = parsed
        .access_token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UNRECOGNIZED_RESPONSE.to_string())?;
    let refresh_token = parsed
        .refresh_token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| RELOGIN_REQUIRED.to_string())?;
    let claims = parsed
        .id_token
        .as_deref()
        .and_then(jwt_payload)
        .or_else(|| jwt_payload(&access_token))
        .ok_or_else(|| UNRECOGNIZED_RESPONSE.to_string())?;
    let id = claims_sub(&claims).ok_or_else(|| UNRECOGNIZED_RESPONSE.to_string())?;
    let account = XaiAccount {
        id,
        label: claims_label(&claims),
        tokens: XaiTokens {
            access_token,
            refresh_token,
            expires_at_ms: now_ms
                + parsed
                    .expires_in
                    .unwrap_or(DEFAULT_TOKEN_LIFETIME_MS / 1000)
                    * 1000,
        },
        updated_at: now_ms,
    };
    let (mut file, revision) = load_file(root)?;
    file.accounts.retain(|existing| existing.id != account.id);
    file.accounts.push(account);
    if file.default_id.is_none() {
        file.default_id = file.accounts.last().map(|account| account.id.clone());
    }
    save_file(root, &file, &revision)?;
    let _ = fs::remove_file(pending_path(root));
    Ok(XaiPollOutcome::Completed)
}

pub(crate) fn cancel_login(root: &Path) -> bool {
    let _guard = lock();
    fs::remove_file(pending_path(root)).is_ok()
}

pub(crate) fn delete_account(root: &Path, id: &str, expected_revision: &str) -> Result<(), String> {
    let _guard = lock()?;
    let (mut file, revision) = load_file(root)?;
    if revision != expected_revision {
        return Err("xAI 账号库已变化，请刷新后重试".into());
    }
    let before = file.accounts.len();
    file.accounts.retain(|account| account.id != id);
    if file.accounts.len() == before {
        return Err("xAI 账号不存在，请刷新后重试".into());
    }
    file.bindings.retain(|_, bound| bound != id);
    if file.default_id.as_deref() == Some(id) {
        file.default_id = file.accounts.first().map(|account| account.id.clone());
    }
    save_file(root, &file, expected_revision)?;
    Ok(())
}

/// Binds (or unbinds with `None`) one Codex profile to one xAI account.
pub(crate) fn set_profile_binding(
    root: &Path,
    profile_id: &str,
    account_id: Option<&str>,
    expected_revision: &str,
) -> Result<String, String> {
    let _guard = lock()?;
    let (mut file, revision) = load_file(root)?;
    if revision != expected_revision {
        return Err("xAI 账号库已变化，请刷新后重试".into());
    }
    match account_id {
        Some(id) => {
            if !file.accounts.iter().any(|account| account.id == id) {
                return Err("xAI 账号不存在，请刷新后重试".into());
            }
            file.bindings.insert(profile_id.to_string(), id.to_string());
        }
        None => {
            file.bindings.remove(profile_id);
        }
    }
    save_file(root, &file, expected_revision)
}

/// Resolves a fresh access token for one Codex profile: the bound account
/// wins, otherwise the default account. An expiring or expired token is
/// refreshed with the stored grant; a rejected refresh fails with a
/// re-login requirement instead of a stale token.
pub(crate) fn valid_token_for_profile(
    root: &Path,
    profile_id: &str,
) -> Result<XaiResolution, String> {
    valid_token_for_profile_at(
        root,
        profile_id,
        DISCOVERY_URL,
        chrono::Utc::now().timestamp_millis(),
    )
}

pub(crate) fn valid_token_for_profile_at(
    root: &Path,
    profile_id: &str,
    discovery_url: &str,
    now_ms: i64,
) -> Result<XaiResolution, String> {
    let _guard = lock()?;
    let (mut file, revision) = load_file(root)?;
    let bound = file.bindings.get(profile_id).cloned();
    let account_id = bound.or_else(|| file.default_id.clone());
    let Some(account_id) = account_id else {
        return Err("尚未登录任何 xAI 账号；请先在认证面板完成 xAI 登录".into());
    };
    let index = file
        .accounts
        .iter()
        .position(|account| account.id == account_id)
        .ok_or_else(|| RELOGIN_REQUIRED.to_string())?;
    if file.accounts[index].tokens.expires_at_ms - now_ms > REFRESH_BUFFER_MS {
        return Ok(XaiResolution {
            account_id,
            access_token: file.accounts[index].tokens.access_token.clone(),
        });
    }
    // Refresh through the issuer's current token endpoint.
    let endpoints = resolve_endpoints(discovery_url)?;
    let form = [
        ("grant_type", "refresh_token".to_string()),
        (
            "refresh_token",
            file.accounts[index].tokens.refresh_token.clone(),
        ),
        ("client_id", CLIENT_ID.to_string()),
    ]
    .map(|(key, value)| format!("{key}={}", crate::official_login::percent_encode(&value)))
    .join("&");
    let (status, body) = http_request(
        "POST",
        &endpoints.token_endpoint,
        &format!(
            "Content-Type: application/x-www-form-urlencoded\r\nAccept: application/json\r\nUser-Agent: {USER_AGENT}\r\n"
        ),
        form.as_bytes(),
    )
    .map_err(|error| format!("xAI 令牌刷新失败：{error}"))?;
    if status == 400 && (body.contains("invalid_grant") || body.contains("invalid_request")) {
        return Err(RELOGIN_REQUIRED.into());
    }
    if status != 200 {
        return Err(format!("xAI 令牌刷新失败：HTTP {status}"));
    }
    #[derive(Deserialize)]
    struct RefreshGrant {
        access_token: Option<String>,
        #[serde(default)]
        refresh_token: Option<String>,
        #[serde(default)]
        expires_in: Option<i64>,
    }
    let parsed: RefreshGrant =
        serde_json::from_str(&body).map_err(|_| UNRECOGNIZED_RESPONSE.to_string())?;
    let access_token = parsed
        .access_token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UNRECOGNIZED_RESPONSE.to_string())?;
    file.accounts[index].tokens.access_token = access_token.clone();
    if let Some(refresh) = parsed.refresh_token.filter(|value| !value.is_empty()) {
        file.accounts[index].tokens.refresh_token = refresh;
    }
    file.accounts[index].tokens.expires_at_ms = now_ms
        + parsed
            .expires_in
            .unwrap_or(DEFAULT_TOKEN_LIFETIME_MS / 1000)
            * 1000;
    file.accounts[index].updated_at = now_ms;
    save_file(root, &file, &revision)?;
    Ok(XaiResolution {
        account_id,
        access_token,
    })
}

/// Account listing for commands; the revision guards binding updates.
pub(crate) fn view(
    root: &Path,
) -> Result<
    (
        Vec<XaiAccount>,
        Option<String>,
        std::collections::BTreeMap<String, String>,
        String,
    ),
    String,
> {
    let (file, revision) = load_file(root)?;
    Ok((file.accounts, file.default_id, file.bindings, revision))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::official_login::fake_oauth::FakeServer;

    /// Two fake servers: the discovery document points the flow at the flow
    /// server, so both address bindings are known before any body is built.
    struct Fixture {
        _discovery: FakeServer,
        _flow: FakeServer,
        discovery_url: String,
    }

    fn fixture(flow_bodies: &[String], discovery_count: usize) -> Fixture {
        let flow = FakeServer::start(flow_bodies);
        let document = format!(
            "{{\"issuer\":\"https://auth.x.ai\",\"token_endpoint\":\"http://{addr}/token\",\"device_authorization_endpoint\":\"http://{addr}/device\"}}",
            addr = flow.address
        );
        let bodies = vec![document; discovery_count];
        let discovery = FakeServer::start(&bodies);
        Fixture {
            discovery_url: format!("http://{}/discovery", discovery.address),
            _discovery: discovery,
            _flow: flow,
        }
    }

    fn device_grant() -> String {
        "{\"device_code\":\"device-1\",\"user_code\":\"XAI-CODE\",\"verification_uri\":\"https://auth.x.ai/device\"}".into()
    }

    fn token_grant(access: &str, refresh: &str, expires_in: i64) -> String {
        let id_token = "h.eyJzdWIiOiJzdWJpIiwiZW1haWwiOiJvbmVAeC5leGFtcGxlIn0.s";
        format!("{{\"access_token\":\"{access}\",\"refresh_token\":\"{refresh}\",\"expires_in\":{expires_in},\"id_token\":\"{id_token}\"}}")
    }

    fn dead_discovery() -> String {
        // Only consumed by code paths that must not run.
        "http://127.0.0.1:1/discovery".into()
    }

    #[test]
    fn device_flow_completes_stores_the_account_and_defaults_to_it() {
        let fixture = fixture(
            &[
                device_grant(),
                ":400:{\"error\":\"authorization_pending\"}".into(),
                token_grant("access-1", "refresh-1", 3600),
            ],
            1,
        );
        let root = tempfile::tempdir().unwrap();
        let session = start_login_at(root.path(), &fixture.discovery_url, 1_000).unwrap();
        assert_eq!(session.user_code, "XAI-CODE");
        assert_eq!(session.verification_uri, "https://auth.x.ai/device");
        assert!(pending_path(root.path()).exists());

        assert!(matches!(
            poll_login_at(root.path(), 2_000).unwrap(),
            XaiPollOutcome::Pending
        ));
        assert!(matches!(
            poll_login_at(root.path(), 2_500).unwrap(),
            XaiPollOutcome::Completed
        ));
        assert!(!pending_path(root.path()).exists());

        let (accounts, default_id, bindings, revision) = view(root.path()).unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, "subi");
        assert_eq!(accounts[0].label, "one@x.example");
        assert_eq!(default_id.as_deref(), Some("subi"));
        assert!(bindings.is_empty());

        set_profile_binding(root.path(), "profile-1", Some("subi"), &revision).unwrap();
        let (_, _, bindings, revision) = view(root.path()).unwrap();
        assert_eq!(bindings.get("profile-1").map(String::as_str), Some("subi"));
        set_profile_binding(root.path(), "profile-1", None, &revision).unwrap();
        let (_, _, bindings, _) = view(root.path()).unwrap();
        assert!(bindings.get("profile-1").is_none());
    }

    #[test]
    fn cancelling_removes_the_pending_session_and_poll_fails_without_one() {
        let fixture = fixture(&[device_grant()], 1);
        let root = tempfile::tempdir().unwrap();
        start_login_at(root.path(), &fixture.discovery_url, 1_000).unwrap();
        assert!(cancel_login(root.path()));
        assert!(!pending_path(root.path()).exists());
        assert!(poll_login_at(root.path(), 2_000)
            .unwrap_err()
            .contains("没有进行中的 xAI 登录"));
    }

    #[test]
    fn denial_and_expiry_fail_with_fixed_messages() {
        let denied = fixture(
            &[device_grant(), ":400:{\"error\":\"access_denied\"}".into()],
            1,
        );
        let root = tempfile::tempdir().unwrap();
        start_login_at(root.path(), &denied.discovery_url, 1_000).unwrap();
        let error = poll_login_at(root.path(), 2_000).unwrap_err();
        assert!(error.contains("拒绝"), "{error}");

        let expired = fixture(
            &[
                "{\"device_code\":\"device-1\",\"user_code\":\"XAI-CODE\",\"verification_uri\":\"https://auth.x.ai/device\",\"expires_in\":10}".into(),
                ":400:{\"error\":\"expired_token\"}".into(),
            ],
            1,
        );
        let root = tempfile::tempdir().unwrap();
        start_login_at(root.path(), &expired.discovery_url, 1_000).unwrap();
        let error = poll_login_at(root.path(), 20_000).unwrap_err();
        assert!(error.contains("过期"), "{error}");
    }

    #[test]
    fn fresh_tokens_resolve_without_the_network_and_expired_ones_refresh() {
        let fixture = fixture(
            &[
                device_grant(),
                token_grant("access-1", "refresh-1", 3600),
                token_grant("access-2", "refresh-2", 3600),
            ],
            2,
        );
        let root = tempfile::tempdir().unwrap();
        start_login_at(root.path(), &fixture.discovery_url, 1_000).unwrap();
        poll_login_at(root.path(), 1_500).unwrap();

        // Fresh: resolves without touching the discovery endpoint.
        let fresh =
            valid_token_for_profile_at(root.path(), "profile-1", &dead_discovery(), 2_000).unwrap();
        assert_eq!(fresh.account_id, "subi");
        assert_eq!(fresh.access_token, "access-1");

        // 41 seconds before expiry sits inside the refresh buffer.
        let refreshed =
            valid_token_for_profile_at(root.path(), "profile-1", &fixture.discovery_url, 3_560_000)
                .unwrap();
        assert_eq!(refreshed.access_token, "access-2");
        let (accounts, _, _, _) = view(root.path()).unwrap();
        assert_eq!(accounts[0].tokens.refresh_token, "refresh-2");
    }

    #[test]
    fn rejected_refresh_requires_relogin_and_missing_accounts_fail_closed() {
        let fixture = fixture(
            &[
                device_grant(),
                token_grant("access-1", "refresh-1", 3600),
                ":400:{\"error\":\"invalid_grant\"}".into(),
            ],
            2,
        );
        let root = tempfile::tempdir().unwrap();
        start_login_at(root.path(), &fixture.discovery_url, 1_000).unwrap();
        poll_login_at(root.path(), 1_500).unwrap();
        // 41 seconds before expiry: inside the refresh buffer.
        let error =
            valid_token_for_profile_at(root.path(), "profile-1", &fixture.discovery_url, 3_560_000)
                .unwrap_err();
        assert!(error.contains("重新登录"), "{error}");

        let empty = tempfile::tempdir().unwrap();
        let error = valid_token_for_profile_at(empty.path(), "profile-1", &dead_discovery(), 1_000)
            .unwrap_err();
        assert!(error.contains("尚未登录"), "{error}");
    }
}
