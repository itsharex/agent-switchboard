use serde_json::Value;
use std::collections::BTreeMap;

use crate::contracts::{
    ClaudeApiKeyField, LocalProxyRequestOverrides, ProviderAuthBinding, ProviderConnectionOptions,
    ProviderEndpoint,
};

use crate::ccswitch::row::{CcSwitchProposal, CcSwitchRow, CcSwitchSkip};

/// Maps one source row into a proposal or a skip.
pub fn map_row(row: &CcSwitchRow) -> Result<CcSwitchProposal, CcSwitchSkip> {
    let key = format!("{}:{}", row.app_type, row.id);
    match row.app_type.as_str() {
        "codex" => crate::ccswitch::codex::map_codex(key.clone(), row)
            .map_err(|reason| skip_with(key, row, reason)),
        "claude" => super::claude::map_claude(key.clone(), row)
            .map_err(|reason| skip_with(key, row, reason)),
        other => Err(skip_with(
            key,
            row,
            format!("客户端 {other} 超出本应用支持范围"),
        )),
    }
}

fn skip_with(key: String, row: &CcSwitchRow, reason: String) -> CcSwitchSkip {
    CcSwitchSkip {
        key,
        app_type: row.app_type.clone(),
        name: row.name.clone(),
        reason,
    }
}

pub(crate) fn parse_meta(raw: Option<&str>, warnings: &mut Vec<String>) -> Result<Value, String> {
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return Ok(Value::Null);
    };
    let meta: Value = serde_json::from_str(raw).map_err(|_| "meta 无法解析".to_string())?;
    if !meta.is_object() {
        warnings.push("未导入: meta（格式无效）".to_string());
        return Ok(Value::Null);
    }
    Ok(meta)
}

pub(crate) fn map_connection(
    meta: &Value,
    warnings: &mut Vec<String>,
) -> ProviderConnectionOptions {
    let Some(meta) = meta.as_object() else {
        return ProviderConnectionOptions::default();
    };
    for name in meta.keys() {
        if ![
            "apiFormat",
            "usage_script",
            "usageScript",
            "isFullUrl",
            "is_full_url",
            "customEndpoints",
            "custom_endpoints",
            "endpointAutoSelect",
            "endpoint_auto_select",
            "customUserAgent",
            "custom_user_agent",
            "localProxyRequestOverrides",
            "local_proxy_request_overrides",
            "authBinding",
            "auth_binding",
            "providerType",
            "provider_type",
            "apiKeyField",
            "api_key_field",
        ]
        .contains(&name.as_str())
        {
            warnings.push(format!("未导入: meta.{name}"));
        }
    }
    let is_full_url = first_field(meta, "isFullUrl", "is_full_url");
    let endpoint_auto_select = first_field(meta, "endpointAutoSelect", "endpoint_auto_select");
    let custom_user_agent = first_field(meta, "customUserAgent", "custom_user_agent");
    let auth_binding = first_field(meta, "authBinding", "auth_binding");
    let provider_type = first_field(meta, "providerType", "provider_type");
    let api_key_field = first_field(meta, "apiKeyField", "api_key_field");
    let mut options = ProviderConnectionOptions {
        is_full_url: bool_field(is_full_url, "meta.isFullUrl", warnings).unwrap_or(false),
        endpoint_auto_select: bool_field(endpoint_auto_select, "meta.endpointAutoSelect", warnings),
        custom_user_agent: string_field(custom_user_agent, "meta.customUserAgent", warnings),
        auth_binding: map_auth_binding(auth_binding, warnings),
        provider_type: string_field(provider_type, "meta.providerType", warnings),
        api_key_field: map_api_key_field(api_key_field, warnings),
        ..ProviderConnectionOptions::default()
    };
    options.custom_endpoints = map_custom_endpoints(
        first_field(meta, "customEndpoints", "custom_endpoints"),
        warnings,
    );
    options.local_proxy_request_overrides = map_request_overrides(
        first_field(
            meta,
            "localProxyRequestOverrides",
            "local_proxy_request_overrides",
        ),
        warnings,
    );
    options
}

fn first_field<'a>(
    object: &'a serde_json::Map<String, Value>,
    preferred: &str,
    fallback: &str,
) -> Option<&'a Value> {
    object.get(preferred).or_else(|| object.get(fallback))
}

fn bool_field(value: Option<&Value>, name: &str, warnings: &mut Vec<String>) -> Option<bool> {
    let Some(value) = value else {
        return None;
    };
    match value.as_bool() {
        Some(value) => Some(value),
        None => {
            warnings.push(format!("未导入: {name}（格式无效）"));
            None
        }
    }
}

fn string_field(value: Option<&Value>, name: &str, warnings: &mut Vec<String>) -> Option<String> {
    let Some(value) = value else {
        return None;
    };
    match value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => Some(value.to_string()),
        None => {
            warnings.push(format!("未导入: {name}"));
            None
        }
    }
}

fn map_api_key_field(
    value: Option<&Value>,
    warnings: &mut Vec<String>,
) -> Option<ClaudeApiKeyField> {
    match value.and_then(Value::as_str) {
        None => None,
        Some("ANTHROPIC_AUTH_TOKEN") => Some(ClaudeApiKeyField::AnthropicAuthToken),
        Some("ANTHROPIC_API_KEY") => Some(ClaudeApiKeyField::AnthropicApiKey),
        Some(_) => {
            warnings.push("未导入: meta.apiKeyField".to_string());
            None
        }
    }
}

fn map_auth_binding(
    value: Option<&Value>,
    warnings: &mut Vec<String>,
) -> Option<ProviderAuthBinding> {
    let Some(object) = value.and_then(Value::as_object) else {
        if value.is_some() {
            warnings.push("未导入: meta.authBinding".to_string());
        }
        return None;
    };
    let source = object
        .get("source")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let Some(source) = source else {
        warnings.push("未导入: meta.authBinding.source".to_string());
        return None;
    };
    Some(ProviderAuthBinding {
        source,
        auth_provider: object
            .get("authProvider")
            .or_else(|| object.get("auth_provider"))
            .and_then(Value::as_str)
            .map(str::to_string),
        account_id: object
            .get("accountId")
            .or_else(|| object.get("account_id"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn map_custom_endpoints(
    value: Option<&Value>,
    warnings: &mut Vec<String>,
) -> BTreeMap<String, ProviderEndpoint> {
    let Some(entries) = value.and_then(Value::as_object) else {
        if value.is_some() {
            warnings.push("未导入: meta.custom_endpoints".to_string());
        }
        return BTreeMap::new();
    };
    entries
        .iter()
        .filter_map(|(key, entry)| {
            let object = entry.as_object()?;
            let url = object
                .get("url")
                .and_then(Value::as_str)
                .or_else(|| Some(key.as_str()))?;
            let url = url.trim();
            if url.is_empty() {
                warnings.push(format!("未导入: meta.custom_endpoints.{key}"));
                return None;
            }
            let added_at = object
                .get("addedAt")
                .or_else(|| object.get("added_at"))
                .and_then(Value::as_i64)
                .unwrap_or_default();
            let last_used = object
                .get("lastUsed")
                .or_else(|| object.get("last_used"))
                .and_then(Value::as_i64);
            Some((
                url.to_string(),
                ProviderEndpoint {
                    url: url.to_string(),
                    added_at,
                    last_used,
                },
            ))
        })
        .collect()
}

fn map_request_overrides(
    value: Option<&Value>,
    warnings: &mut Vec<String>,
) -> Option<LocalProxyRequestOverrides> {
    let Some(object) = value.and_then(Value::as_object) else {
        if value.is_some() {
            warnings.push("未导入: meta.localProxyRequestOverrides".to_string());
        }
        return None;
    };
    let mut headers = BTreeMap::new();
    if let Some(values) = object.get("headers").and_then(Value::as_object) {
        for (name, value) in values {
            if let Some(value) = value.as_str() {
                headers.insert(name.clone(), value.to_string());
            } else {
                warnings.push(format!(
                    "未导入: meta.localProxyRequestOverrides.headers.{name}"
                ));
            }
        }
    } else if object.get("headers").is_some() {
        warnings.push("未导入: meta.localProxyRequestOverrides.headers".to_string());
    }
    let body = object.get("body").cloned().unwrap_or(Value::Null);
    if !body.is_null() && !body.is_object() {
        warnings.push("未导入: meta.localProxyRequestOverrides.body".to_string());
    }
    Some(LocalProxyRequestOverrides {
        headers,
        body: body.is_object().then_some(body).unwrap_or(Value::Null),
    })
}
