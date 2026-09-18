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

use crate::probe::http_request;

const DISCOVERY_URL: &str = "https://auth.x.ai/.well-known/openid-configuration";
const CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
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
    }
    let parsed: Discovery =
        serde_json::from_str(&body).map_err(|_| UNRECOGNIZED_RESPONSE.to_string())?;
    // Loopback endpoints exist only for isolated flow tests.
    let trusted = |value: &Option<String>| {
        value.as_deref().is_some_and(|value| {
            value.starts_with("https://") || value.starts_with("http://127.0.0.1:")
        })
    };
    if !trusted(&parsed.token_endpoint) {
        return Err(UNRECOGNIZED_RESPONSE.to_string());
    }
    Ok(XaiEndpoints {
        token_endpoint: parsed.token_endpoint.unwrap(),
    })
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
            "{{\"issuer\":\"https://auth.x.ai\",\"token_endpoint\":\"http://{addr}/token\"}}",
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

    fn token_grant(access: &str, refresh: &str, expires_in: i64) -> String {
        format!("{{\"access_token\":\"{access}\",\"refresh_token\":\"{refresh}\",\"expires_in\":{expires_in}}}")
    }

    fn dead_discovery() -> String {
        // Only consumed by code paths that must not run.
        "http://127.0.0.1:1/discovery".into()
    }

    /// Seeds one default account directly in the store; refresh-path tests
    /// no longer travel through the (removed) device-login flow.
    fn seeded_root(access: &str, refresh: &str, expires_at_ms: i64) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let file = XaiFile {
            version: 1,
            default_id: Some("subi".into()),
            accounts: vec![XaiAccount {
                id: "subi".into(),
                label: "one@x.example".into(),
                tokens: XaiTokens {
                    access_token: access.into(),
                    refresh_token: refresh.into(),
                    expires_at_ms,
                },
                updated_at: 1_000,
            }],
            bindings: Default::default(),
        };
        save_file(root.path(), &file, &sha256_hex("")).unwrap();
        root
    }

    #[test]
    fn fresh_tokens_resolve_without_the_network_and_expired_ones_refresh() {
        let fixture = fixture(&[token_grant("access-2", "refresh-2", 3600)], 1);
        // Expires at 1_000 + 3_600_000.
        let root = seeded_root("access-1", "refresh-1", 3_601_000);

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
        let (file, _) = load_file(root.path()).unwrap();
        assert_eq!(file.accounts[0].tokens.refresh_token, "refresh-2");
    }

    #[test]
    fn rejected_refresh_requires_relogin_and_missing_accounts_fail_closed() {
        let fixture = fixture(&[":400:{\"error\":\"invalid_grant\"}".into()], 1);
        let root = seeded_root("access-1", "refresh-1", 3_601_000);
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
