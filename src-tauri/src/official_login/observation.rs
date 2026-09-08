//! Read-only observation of the file-backed Codex account. No token refresh or repair.
use std::io::ErrorKind;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexLoginState {
    Configured,
    Missing,
    ApiKey,
    Invalid,
    Unreadable,
    UnsupportedStorage,
}

impl CodexLoginState {
    pub(crate) fn require(self) -> Result<(), String> {
        match self {
            Self::Configured => Ok(()),
            Self::Missing => Err("使用 Codex 第三方前，请先完成官方登录".into()),
            Self::ApiKey => Err("Codex 当前处于 API-key 登录模式，请重新完成官方登录；切换不会改写登录缓存".into()),
            Self::Invalid => Err("Codex 官方登录存储无效或不完整，请重新完成官方登录".into()),
            Self::Unreadable => Err("无法读取 Codex 配置或官方登录存储".into()),
            Self::UnsupportedStorage => Err("当前仅支持 Codex 文件官方登录；keyring、auto 和 ephemeral 存储尚不受支持，请显式使用 file 存储后重新登录".into()),
        }
    }
}

pub(crate) fn parse_codex_login(auth: &str) -> CodexLoginState {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(auth) else {
        return CodexLoginState::Invalid;
    };
    if value.get("auth_mode").and_then(|v| v.as_str()) == Some("apikey")
        || value.get("OPENAI_API_KEY").is_some_and(|v| !v.is_null())
    {
        return CodexLoginState::ApiKey;
    }
    if value.get("auth_mode").and_then(|v| v.as_str()) != Some("chatgpt") {
        return CodexLoginState::Invalid;
    }
    let Some(tokens) = value.get("tokens").and_then(|v| v.as_object()) else {
        return CodexLoginState::Invalid;
    };
    if ["access_token", "refresh_token", "id_token"]
        .iter()
        .all(|key| {
            tokens
                .get(*key)
                .and_then(|v| v.as_str())
                .is_some_and(|v| !v.trim().is_empty())
        })
    {
        CodexLoginState::Configured
    } else {
        CodexLoginState::Invalid
    }
}

pub(crate) fn observe_codex_login(config_path: &Path) -> CodexLoginState {
    let config = match std::fs::read_to_string(config_path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(_) => return CodexLoginState::Unreadable,
    };
    observe_codex_login_for_config(config_path, &config)
}

/// Checks the candidate storage mode before a configuration backup is restored.
pub(crate) fn observe_codex_login_for_config(config_path: &Path, config: &str) -> CodexLoginState {
    let Ok(document) = config.parse::<toml_edit::DocumentMut>() else {
        return CodexLoginState::Invalid;
    };
    match document.get("cli_auth_credentials_store") {
        None => {} // Codex's current default is the file store.
        Some(value) if value.as_str() == Some("file") => {}
        Some(_) => return CodexLoginState::UnsupportedStorage,
    }
    let Some(parent) = config_path.parent() else {
        return CodexLoginState::Unreadable;
    };
    match std::fs::read_to_string(parent.join("auth.json")) {
        Ok(auth) => parse_codex_login(&auth),
        Err(error) if error.kind() == ErrorKind::NotFound => CodexLoginState::Missing,
        Err(_) => CodexLoginState::Unreadable,
    }
}

pub(crate) fn require_codex_login(config_path: &Path) -> Result<(), String> {
    observe_codex_login(config_path).require()
}

#[cfg(test)]
mod tests {
    use super::*;
    const AUTH: &str = r#"{"auth_mode":"chatgpt","tokens":{"access_token":"test-access","refresh_token":"test-refresh","id_token":"test-id"}}"#;

    #[test]
    fn storage_selection_and_invalid_modes_never_repair_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let auth = dir.path().join("auth.json");
        assert_eq!(observe_codex_login(&config), CodexLoginState::Missing);
        std::fs::write(&auth, AUTH).unwrap();
        assert_eq!(observe_codex_login(&config), CodexLoginState::Configured);
        for store in ["keyring", "auto", "ephemeral"] {
            std::fs::write(
                &config,
                format!("cli_auth_credentials_store = \"{store}\"\n"),
            )
            .unwrap();
            assert_eq!(
                observe_codex_login(&config),
                CodexLoginState::UnsupportedStorage
            );
            assert_eq!(std::fs::read_to_string(&auth).unwrap(), AUTH);
        }
        std::fs::write(&config, "cli_auth_credentials_store = \"file\"\n").unwrap();
        std::fs::write(&auth, AUTH.replace("chatgpt", "apikey")).unwrap();
        assert_eq!(observe_codex_login(&config), CodexLoginState::ApiKey);
        std::fs::write(&auth, "not json").unwrap();
        assert_eq!(observe_codex_login(&config), CodexLoginState::Invalid);
        assert_eq!(std::fs::read_to_string(&auth).unwrap(), "not json");
    }

    #[test]
    fn oauth_residue_is_not_an_official_login() {
        assert_eq!(
            parse_codex_login(&AUTH.replace("chatgpt", "apikey")),
            CodexLoginState::ApiKey
        );
        assert_eq!(
            parse_codex_login(r#"{"tokens":{"access_token":"old"}}"#),
            CodexLoginState::Invalid
        );
        assert_eq!(
            parse_codex_login(r#"{"auth_mode":"chatgpt","tokens":{"access_token":"old"}}"#),
            CodexLoginState::Invalid
        );
    }
}
