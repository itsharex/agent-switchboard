//! Claude-only provider validation, including managed accounts and request-side metadata.

use super::ValidationError;
use crate::contracts::{
    AppKind, AuthenticationScheme, ProviderConnectionOptions, UpstreamProtocol,
};

pub(super) fn validate_connection(
    app: AppKind,
    connection: &ProviderConnectionOptions,
    protocol: UpstreamProtocol,
    authentication: Option<AuthenticationScheme>,
    api_key: &str,
) -> Result<bool, ValidationError> {
    let error = ValidationError::ClaudeProviderOptions;
    connection.validate_models_url().map_err(error)?;
    if app != AppKind::Claude {
        if protocol == UpstreamProtocol::GeminiGenerateContent {
            return Err(error("Gemini Native 是 Claude 专用上游".into()));
        }
        if connection.claude_billing.is_some()
            || connection.claude_prompt_cache_key.is_some()
        {
            return Err(error("Claude 专用连接设置不能用于 Codex".into()));
        }
        return Ok(false);
    }
    if protocol == UpstreamProtocol::GeminiGenerateContent {
        crate::claude_gemini::credential(api_key, authentication).map_err(error)?;
    } else if authentication == Some(AuthenticationScheme::XGoogApiKey) {
        return Err(error("x-goog-api-key 仅用于 Gemini Native".into()));
    }
    if let Some(billing) = &connection.claude_billing {
        billing.validate().map_err(error)?;
    }
    if let Some(key) = &connection.claude_prompt_cache_key {
        if key.trim().is_empty() || key.len() > 512 || key.chars().any(char::is_control) {
            return Err(error("Claude 提示缓存键格式无效".into()));
        }
    }
    let managed = crate::claude_auth::managed_auth(connection).map_err(error)?;
    if let Some((provider, _)) = managed {
        provider.validate_protocol(protocol).map_err(error)?;
        if authentication.is_some_and(|scheme| scheme != AuthenticationScheme::Bearer) {
            return Err(error("Claude 托管账号使用 Bearer 认证".into()));
        }
        if !api_key.is_empty() {
            return Err(error("Claude 托管账号不应同时保存供应商 API Key".into()));
        }
    }
    Ok(managed.is_some())
}

pub(super) fn validate_native(
    app: AppKind,
    native: &crate::claude_native::ClaudeNative,
    base: Option<&str>,
    connection: &ProviderConnectionOptions,
    api_key: &str,
    authentication: Option<AuthenticationScheme>,
    protocol: Option<UpstreamProtocol>,
    protocol_options: bool,
) -> Result<(), ValidationError> {
    let error = ValidationError::ClaudeProviderOptions;
    if app != AppKind::Claude
        || protocol != Some(UpstreamProtocol::AnthropicMessages)
        || authentication.is_some()
        || !api_key.is_empty()
        || protocol_options
    {
        return Err(error(
            "Claude 原生云模式使用自己的 SDK 认证，不接受普通 API 密钥、认证头或跨协议选项".into(),
        ));
    }
    if connection.requires_gateway(base)
        || connection.auth_binding.is_some()
        || connection.models_url.is_some()
        || !connection.custom_endpoints.is_empty()
    {
        return Err(error(
            "Claude 原生云 SDK 不能使用本机 HTTP 网关覆盖、托管账号、端点自动选择或请求计费".into(),
        ));
    }
    native.validate(base).map_err(error)
}
