use crate::provider_diagnostics::{
    http_diagnostic, network_diagnostic, read_http_diagnostic, ProviderFailureKind,
};
use asb_core::contracts::UpstreamProtocol;
use serde::Serialize;
use std::time::Duration;

/// One model from the provider's configured API root: the requestable
/// id plus the optional `owned_by` vendor used to group the picker menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModel {
    pub id: String,
    pub owned_by: Option<String>,
}

/// Fetches the model list beneath the same API root as model requests, with the exact
/// protocol-derived authentication scheme. The profile API key travels only in
/// the selected request header and is never logged or echoed in errors.
/// Nothing is cached.
pub fn fetch_models(
    base_url: &str,
    api_key: &str,
    protocol: UpstreamProtocol,
) -> Result<Vec<ProviderModel>, String> {
    let url = asb_core::endpoint::models_endpoint(base_url, protocol)?;
    let client = reqwest::blocking::Client::builder()
        .user_agent("Agent Switchboard")
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法初始化模型列表请求".to_string())?;
    let request = match protocol.authentication_scheme() {
        asb_core::AuthenticationScheme::Bearer => client.get(&url).bearer_auth(api_key),
        asb_core::AuthenticationScheme::XApiKey => client
            .get(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01"),
    };
    let response = request
        .send()
        .map_err(|error| network_diagnostic(&url, &error).summary())?;
    if !response.status().is_success() {
        return Err(read_http_diagnostic(response, &[api_key]).summary());
    }
    let mut diagnostic = http_diagnostic(
        &url,
        response.status().as_u16(),
        response.headers(),
        &[],
        false,
        &[api_key],
    );
    diagnostic.kind = ProviderFailureKind::ResponseParse;
    diagnostic.message = "模型列表响应无法解析".to_string();
    let body = response.bytes().map_err(|error| {
        let mut failure = network_diagnostic(&url, &error);
        failure.status = diagnostic.status;
        failure.request_id = diagnostic.request_id.clone();
        failure.summary()
    })?;
    parse_models(&String::from_utf8_lossy(&body)).map_err(|error| {
        diagnostic.message = error;
        diagnostic.summary()
    })
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
