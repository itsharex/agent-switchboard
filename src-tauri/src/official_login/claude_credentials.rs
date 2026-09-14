//! Claude native credential ownership, independent from Codex authentication.

use crate::config_store::write_json_atomic;
use serde_json::{json, Value};
use std::path::Path;
const EXISTING_READ_ERROR: &str = "无法读取现有 Claude 登录缓存";

/// Tokens from one Claude token exchange.
pub(crate) struct ClaudeTokens {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) expires_in: i64,
    pub(crate) scope: String,
}

impl std::fmt::Debug for ClaudeTokens {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ClaudeTokens")
            .field("access_token", &asb_core::redact::REDACTED)
            .field("refresh_token", &asb_core::redact::REDACTED)
            .field("expires_in", &self.expires_in)
            .field("scope", &self.scope)
            .finish()
    }
}

pub(crate) fn write_claude_credentials(path: &Path, tokens: &ClaudeTokens) -> Result<(), String> {
    if tokens.expires_in <= 0
        || [&tokens.access_token, &tokens.refresh_token]
            .iter()
            .any(|value| value.trim().is_empty() || value.chars().any(char::is_control))
    {
        return Err("Claude 登录服务返回了无效凭据，未修改登录缓存".into());
    }
    let mut root = match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .ok_or_else(|| {
                "现有 Claude 登录缓存格式无效；请备份并修复后重试，原始文件未改写".to_string()
            })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::Map::new(),
        Err(_) => return Err(EXISTING_READ_ERROR.into()),
    };
    let expires_at = chrono::Utc::now()
        .timestamp_millis()
        .saturating_add(tokens.expires_in.saturating_mul(1000));
    let scopes: Vec<&str> = tokens.scope.split_whitespace().collect();
    // The token response does not establish a subscription tier. Claude owns
    // discovery of the actual plan; never manufacture a Max subscription.
    root.insert(
        "claudeAiOauth".into(),
        json!({"accessToken":tokens.access_token,
        "refreshToken":tokens.refresh_token, "expiresAt":expires_at, "scopes":scopes}),
    );
    let text =
        serde_json::to_string_pretty(&root).map_err(|_| "Claude 登录凭据无法序列化".to_string())?;
    write_json_atomic(path, &text).map_err(|e| format!("Claude 登录凭据写入失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn claude_write_replaces_the_oauth_object_and_keeps_sibling_keys() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join(".credentials.json");
        fs::write(
            &path,
            "{\"claudeAiOauth\":{\"accessToken\":\"old\"},\"host_note\":\"keep\"}",
        )
        .expect("seed");

        write_claude_credentials(
            &path,
            &ClaudeTokens {
                access_token: "at".to_string(),
                refresh_token: "rt".to_string(),
                expires_in: 3600,
                scope: "user:profile user:inference".to_string(),
            },
        )
        .expect("write");

        let merged: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("valid json");
        assert_eq!(merged.get("host_note"), Some(&json!("keep")));
        let oauth = merged.get("claudeAiOauth").expect("oauth object");
        assert_eq!(oauth["accessToken"], json!("at"));
        assert_eq!(oauth["refreshToken"], json!("rt"));
        assert!(oauth.get("subscriptionType").is_none());
        assert_eq!(oauth["scopes"], json!(["user:profile", "user:inference"]));
        assert!(oauth["expiresAt"].as_i64().expect("epoch millis") > 0);
    }

    #[test]
    fn claude_write_fails_closed_when_the_existing_cache_is_unreadable() {
        let directory = tempfile::tempdir().expect("temporary directory");
        // A directory at the cache path makes the existing-file read fail with
        // a non-NotFound error, which must abort the write instead of
        // silently replacing the cache.
        let blocked = directory.path().join(".credentials.json");
        std::fs::create_dir(&blocked).expect("seed directory");

        let outcome = write_claude_credentials(
            &blocked,
            &ClaudeTokens {
                access_token: "at".to_string(),
                refresh_token: "rt".to_string(),
                expires_in: 3600,
                scope: String::new(),
            },
        );

        assert_eq!(outcome, Err(EXISTING_READ_ERROR.to_string()));
        assert!(blocked.is_dir());
    }

    #[test]
    fn malformed_native_credentials_are_preserved_and_invalid_tokens_are_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".credentials.json");
        fs::write(&path, "{broken").unwrap();
        let tokens = ClaudeTokens {
            access_token: "test".into(),
            refresh_token: "refresh".into(),
            expires_in: 3600,
            scope: "user:profile".into(),
        };
        assert!(write_claude_credentials(&path, &tokens).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "{broken");
    }
}
