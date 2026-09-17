//! Claude source connection decoding: auth, protocol, endpoint and executable metadata.

use super::auth;
use crate::ccswitch::mapping::{map_connection, parse_meta};
use crate::contracts::{
    AuthenticationScheme, ClaudeApiKeyField, ProviderConnectionOptions, UpstreamProtocol,
};
use serde_json::Value;

pub(super) struct SourceRoute {
    pub base_url: String,
    pub connection: ProviderConnectionOptions,
    pub authentication: AuthenticationScheme,
    pub api_key: String,
    pub protocol: UpstreamProtocol,
}

pub(super) fn meta(raw: Option<&str>, warnings: &mut Vec<String>) -> Result<Value, String> {
    let value = parse_meta(raw, warnings)?;
    if raw.is_some_and(|raw| !raw.trim().is_empty()) && !value.is_object() {
        return Err("Claude 来源 meta 必须是 JSON 对象".into());
    }
    Ok(value)
}

pub(super) fn route(
    config: &Value,
    env: &serde_json::Map<String, Value>,
    meta: &Value,
    warnings: &mut Vec<String>,
) -> Result<Option<SourceRoute>, String> {
    if let Some((native, base)) = crate::claude_native::from_config(config)? {
        if source_protocol(meta, config, UpstreamProtocol::AnthropicMessages)?
            != UpstreamProtocol::AnthropicMessages
        {
            return Err("Claude 原生云 SDK 不能使用跨协议 apiFormat".into());
        }
        let mut connection = claude_connection(meta, warnings)?;
        connection.claude_native = Some(native);
        return Ok(Some(SourceRoute {
            base_url: base.unwrap_or_default(),
            connection,
            authentication: AuthenticationScheme::Bearer,
            api_key: String::new(),
            protocol: UpstreamProtocol::AnthropicMessages,
        }));
    }
    let base = env
        .get("ANTHROPIC_BASE_URL")
        .and_then(Value::as_str)
        .or_else(|| config.get("base_url").and_then(Value::as_str))
        .or_else(|| config.get("baseURL").and_then(Value::as_str))
        .or_else(|| config.get("apiEndpoint").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut connection = claude_connection(meta, warnings)?;
    auth::import(meta, base, &mut connection)?;
    let managed = crate::claude_auth::managed_auth(&connection)?;
    let Some(base_url) = base.or_else(|| managed.map(|(kind, _)| kind.default_endpoint())) else {
        return Ok(None);
    };
    let fallback = managed
        .map(|(kind, _)| kind.default_protocol())
        .unwrap_or(UpstreamProtocol::AnthropicMessages);
    let protocol = source_protocol(meta, config, fallback)?;
    if let Some((kind, _)) = managed {
        kind.validate_protocol(protocol)?;
    }
    let (mut authentication, api_key) = if managed.is_some() {
        (AuthenticationScheme::Bearer, String::new())
    } else {
        source_credential(env, connection.api_key_field)?
    };
    if protocol == UpstreamProtocol::GeminiGenerateContent {
        authentication = if api_key.starts_with("ya29.") || api_key.trim_start().starts_with('{') {
            AuthenticationScheme::Bearer
        } else {
            AuthenticationScheme::XGoogApiKey
        };
    }
    if let Some(mode) = config.get("auth_mode") {
        if mode.as_str() != Some("bearer_only") {
            return Err("Claude 来源 auth_mode 不受支持".into());
        }
        authentication = AuthenticationScheme::Bearer;
    }
    Ok(Some(SourceRoute {
        base_url: base_url.into(),
        connection,
        protocol,
        authentication,
        api_key,
    }))
}

fn source_protocol(
    meta: &Value,
    config: &Value,
    fallback: UpstreamProtocol,
) -> Result<UpstreamProtocol, String> {
    let raw = meta
        .get("apiFormat")
        .or_else(|| meta.get("api_format"))
        .or_else(|| config.get("api_format"));
    let Some(raw) = raw else {
        return match config.get("openrouter_compat_mode") {
            Some(Value::Bool(true)) => Ok(UpstreamProtocol::ChatCompletions),
            Some(Value::Bool(false)) | None => Ok(fallback),
            Some(_) => Err("openrouter_compat_mode 必须是布尔值".into()),
        };
    };
    match raw.as_str().ok_or("Claude 来源 apiFormat 必须是字符串")? {
        "openai_responses" | "responses" => Ok(UpstreamProtocol::Responses),
        "openai_chat" | "chat" | "chat_completions" => Ok(UpstreamProtocol::ChatCompletions),
        "anthropic" | "anthropic_messages" => Ok(UpstreamProtocol::AnthropicMessages),
        "gemini_native" => Ok(UpstreamProtocol::GeminiGenerateContent),
        value => Err(format!("Claude 来源 apiFormat = {value} 不受支持")),
    }
}

fn map_billing(
    meta: &Value,
    connection: &mut crate::contracts::ProviderConnectionOptions,
) -> Result<(), String> {
    use crate::contracts::{ClaudeBilling, ClaudePricingModelSource};
    if [
        "costMultiplier",
        "pricingModelSource",
        "limitDailyUsd",
        "limitMonthlyUsd",
    ]
    .iter()
    .any(|key| meta.get(key).is_some())
    {
        let source = match meta.get("pricingModelSource").and_then(Value::as_str) {
            None | Some("response") => ClaudePricingModelSource::Response,
            Some("request") => ClaudePricingModelSource::Request,
            Some(_) => return Err("meta.pricingModelSource 必须是 request 或 response".into()),
        };
        let text = |key: &str| -> Result<Option<String>, String> {
            meta.get(key)
                .filter(|v| !v.is_null())
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| format!("meta.{key} 必须是十进制字符串"))
                })
                .transpose()
        };
        let billing = ClaudeBilling {
            cost_multiplier: text("costMultiplier")?.unwrap_or_else(|| "1".into()),
            model_source: source,
            daily_limit_usd: text("limitDailyUsd")?,
            monthly_limit_usd: text("limitMonthlyUsd")?,
        };
        billing.validate()?;
        connection.claude_billing = Some(billing);
    }
    connection.claude_prompt_cache_key = meta
        .get("promptCacheKey")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(())
}

fn claude_connection(
    meta: &Value,
    warnings: &mut Vec<String>,
) -> Result<crate::contracts::ProviderConnectionOptions, String> {
    let mut connection_meta = meta.clone();
    if let Some(object) = connection_meta.as_object_mut() {
        for key in [
            "costMultiplier",
            "pricingModelSource",
            "limitDailyUsd",
            "limitMonthlyUsd",
            "promptCacheKey",
            "githubAccountId",
            "codexFastMode",
            "modelsUrl",
            "api_format",
        ] {
            object.remove(key);
        }
    }
    let mut connection = map_connection(&connection_meta, warnings);
    map_billing(meta, &mut connection)?;
    if let Some(url) = meta.get("modelsUrl") {
        let url = url.as_str().ok_or("meta.modelsUrl 必须是完整 URL")?;
        crate::endpoint::validate_full_url(url)?;
        connection.models_url = Some(url.into());
    }
    Ok(connection)
}

fn source_credential(
    env: &serde_json::Map<String, Value>,
    field: Option<ClaudeApiKeyField>,
) -> Result<(AuthenticationScheme, String), String> {
    let text = |name: &str| {
        env.get(name)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    };
    match field {
        Some(ClaudeApiKeyField::AnthropicApiKey) => text("ANTHROPIC_API_KEY")
            .map(|key| (AuthenticationScheme::XApiKey, key))
            .or_else(|| {
                text("ANTHROPIC_AUTH_TOKEN").map(|key| (AuthenticationScheme::Bearer, key))
            }),
        _ => text("ANTHROPIC_AUTH_TOKEN")
            .map(|key| (AuthenticationScheme::Bearer, key))
            .or_else(|| text("ANTHROPIC_API_KEY").map(|key| (AuthenticationScheme::XApiKey, key))),
    }
    .map(|(scheme, key)| (scheme, key.to_string()))
    .ok_or_else(|| "缺少 ANTHROPIC_AUTH_TOKEN 或 ANTHROPIC_API_KEY".to_string())
}
