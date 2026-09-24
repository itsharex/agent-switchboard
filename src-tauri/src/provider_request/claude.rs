//! Claude managed requests stay separate from the generic test-request lifecycle.

use super::ProviderRequestConnection;
use crate::{
    claude_auth::{ClaudeAuth, ResolvedAccount},
    commands::error::CommandError,
    config_store::ConfigStore,
};
use asb_core::{claude_auth::ClaudeAuthProvider, AppKind, AuthenticationScheme};

pub(super) fn resolve(
    store: &ConfigStore,
    mut connection: ProviderRequestConnection,
    prepared: Option<&ResolvedAccount>,
) -> Result<ProviderRequestConnection, CommandError> {
    if connection.app != AppKind::Claude {
        return Ok(connection);
    }
    if connection.connection.claude_native.is_some() {
        return Err(CommandError::keyed(
            "claude-native-sdk-only",
            "errors.misc.claudeNativeSdkOnly",
            "此 Claude 档案使用原生云 SDK，请通过 Claude Code 验证，不能发送普通 HTTP 测试请求",
        ));
    }
    let account = ClaudeAuth::shared(store.state_root())
        .resolve(&connection.connection)
        .map_err(|message| CommandError::new("claude-account-unavailable", message))?;
    if let Some(account) = account {
        account
            .provider
            .validate_protocol(connection.upstream_protocol)
            .map_err(|message| CommandError::new("claude-account-unavailable", message))?;
        if prepared.is_some_and(|expected| {
            expected.account_id != account.account_id
                || expected.upstream_account_id != account.upstream_account_id
                || expected.endpoint != account.endpoint
        }) {
            return Err(CommandError::keyed(
                "claude-account-changed",
                "errors.misc.claudeAccountChanged",
                "Claude 托管账号或目标已变化，请重新准备请求",
            ));
        }
        connection.base_url = account.endpoint.clone();
        connection.connection.is_full_url = false;
        connection.api_key = account.access_token.clone();
        connection.authentication = Some(AuthenticationScheme::Bearer);
        connection.claude_account = Some(account);
    }
    if connection.upstream_protocol == asb_core::UpstreamProtocol::GeminiGenerateContent {
        let (scheme, key) =
            asb_core::claude_gemini::credential(&connection.api_key, connection.authentication)
                .map_err(|message| {
                    CommandError::new("claude-google-credential-invalid", message)
                })?;
        connection.api_key = key;
        connection.authentication = Some(scheme);
    }
    Ok(connection)
}

pub(super) fn response_body(
    connection: &ProviderRequestConnection,
    body: Vec<u8>,
) -> Result<Vec<u8>, String> {
    if connection
        .claude_account
        .as_ref()
        .is_some_and(|account| account.provider == ClaudeAuthProvider::CodexOauth)
    {
        crate::claude_auth::request::completed_response(&body)
    } else {
        Ok(body)
    }
}


pub(super) fn gemini_test_payload(prompt: &str, max: u32) -> serde_json::Value {
    serde_json::json!({"contents":[{"role":"user","parts":[{"text":prompt}]}],"generationConfig":{"maxOutputTokens":max}})
}

pub(super) fn gemini_test_text(value: &serde_json::Value) -> Result<String, String> {
    let candidate = value
        .pointer("/candidates/0")
        .ok_or("Gemini 响应缺少候选消息")?;
    if candidate["finishReason"] != "STOP" {
        return Err("Gemini 未返回完整模型回复".into());
    }
    let parts = candidate
        .pointer("/content/parts")
        .and_then(serde_json::Value::as_array)
        .ok_or("Gemini 响应缺少消息内容")?;
    Ok(parts
        .iter()
        .filter(|part| part.get("thought") != Some(&serde_json::Value::Bool(true)))
        .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
        .collect())
}
