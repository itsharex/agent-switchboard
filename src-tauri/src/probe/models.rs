use super::transport::{http_get, parse_url};
use asb_core::contracts::UpstreamProtocol;
use serde::Serialize;

/// Builds the OpenAI-compatible model-list path for a provider base URL:
/// a base already ending in a `/v{digits}` segment gets `/models` appended,
/// anything else gets `/v1/models`.
pub(super) fn models_path_for(base_url: &str) -> String {
    let mut path = parse_url(base_url)
        .map(|parsed| parsed.path)
        .unwrap_or_else(|| "/".to_string());
    let trimmed = path.trim_end_matches('/');
    let versioned = trimmed.rsplit('/').next().is_some_and(|segment| {
        let digits = segment.strip_prefix('v').unwrap_or("");
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
    });
    if !path.ends_with('/') {
        path.push('/');
    }
    path.push_str(if versioned { "models" } else { "v1/models" });
    path
}

/// One model from the provider's standard `/v1/models` list: the requestable
/// id plus the optional `owned_by` vendor used to group the picker menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModel {
    pub id: String,
    pub owned_by: Option<String>,
}

/// Fetches a provider's standard `GET /v1/models` list with the exact
/// protocol-derived authentication scheme. The profile API key travels only in
/// the selected request header and is never logged or echoed in errors.
/// Nothing is cached.
pub fn fetch_models(
    base_url: &str,
    api_key: &str,
    protocol: UpstreamProtocol,
) -> Result<Vec<ProviderModel>, String> {
    let parsed = parse_url(base_url).ok_or_else(|| "服务地址必须是 http(s) URL".to_string())?;
    let path = models_path_for(base_url);
    let url = format!(
        "{}://{}:{}{path}",
        if parsed.secure { "https" } else { "http" },
        parsed.host,
        parsed.port
    );
    let (status, body) = http_get(&url, &provider_auth_headers(api_key, protocol))?;

    if !(200..300).contains(&status) {
        if status == 401 || status == 403 {
            return Err(format!(
                "服务地址拒绝了 API 密钥（HTTP {status}），请确认密钥仍然有效；可手动填写模型名"
            ));
        }
        return Err(format!(
            "服务地址返回 HTTP {status}，无法获取模型列表；可手动填写模型名"
        ));
    }
    parse_models(&body)
}

/// Builds only the credential headers selected in the provider contract.
/// The Anthropic version header accompanies that protocol alone. The value
/// must never be logged or echoed in diagnostics.
pub(crate) fn provider_auth_headers(credential: &str, protocol: UpstreamProtocol) -> String {
    let authentication = match protocol.authentication_scheme() {
        asb_core::AuthenticationScheme::Bearer => format!("Authorization: Bearer {credential}"),
        asb_core::AuthenticationScheme::XApiKey => format!("x-api-key: {credential}"),
    };
    match protocol {
        UpstreamProtocol::AnthropicMessages => {
            format!("{authentication}\r\nanthropic-version: 2023-06-01")
        }
        UpstreamProtocol::Responses | UpstreamProtocol::ChatCompletions => authentication,
    }
}

/// Extracts `data[].id` plus the optional `data[].owned_by` vendor from an
/// OpenAI-compatible models response. A missing, non-string, or blank
/// `owned_by` stays `None` so the picker groups it under "其他".
pub(super) fn parse_models(text: &str) -> Result<Vec<ProviderModel>, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "模型列表响应不是有效 JSON".to_string())?;
    let data = value
        .get("data")
        .and_then(|data| data.as_array())
        .ok_or_else(|| "模型列表响应缺少 data 数组".to_string())?;
    let mut models: Vec<ProviderModel> = Vec::new();
    for entry in data {
        if let Some(id) = entry.get("id").and_then(|id| id.as_str()) {
            let owned_by = entry
                .get("owned_by")
                .and_then(|vendor| vendor.as_str())
                .map(str::trim)
                .filter(|vendor| !vendor.is_empty())
                .map(str::to_string);
            if !id.is_empty() && !models.iter().any(|seen: &ProviderModel| seen.id == id) {
                models.push(ProviderModel {
                    id: id.to_string(),
                    owned_by,
                });
            }
        }
    }
    if models.is_empty() {
        return Err("模型列表为空".to_string());
    }
    Ok(models)
}
