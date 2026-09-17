use crate::provider_diagnostics::{
    http_diagnostic, network_diagnostic, read_http_diagnostic, ProviderFailureKind,
};
use asb_core::contracts::UpstreamProtocol;
use serde::Serialize;
use std::{io::Read, time::Duration};

/// One model from the provider's configured API root: the requestable
/// id plus the optional `owned_by` vendor used to group the picker menu. When
/// a source explicitly reports input modalities, image_input preserves that
/// fact; absent metadata stays unknown rather than being inferred from the id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModel {
    pub id: String,
    pub owned_by: Option<String>,
    pub image_input: Option<bool>,
}

pub(super) fn diagnostic_secrets(
    api_key: &str,
    connection: &asb_core::contracts::ProviderConnectionOptions,
) -> Vec<String> {
    let mut values = vec![api_key.to_string()];
    if let Some(overrides) = &connection.local_proxy_request_overrides {
        values.extend(overrides.headers.values().filter(|value| !value.is_empty()).cloned());
    }
    if let Some(user_agent) = &connection.custom_user_agent {
        values.push(user_agent.clone());
    }
    values.sort();
    values.dedup();
    values
}

/// Fetches the model list beneath the same API root as model requests, with the exact
/// selected authentication scheme. The profile API key travels only in
/// the selected request header and is never logged or echoed in errors.
/// Nothing is cached.
const MAX_MODELS_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

pub fn fetch_models(
    base_url: &str,
    api_key: &str,
    protocol: UpstreamProtocol,
    authentication: Option<asb_core::AuthenticationScheme>,
    connection: &asb_core::contracts::ProviderConnectionOptions,
) -> Result<Vec<ProviderModel>, String> {
    if protocol == UpstreamProtocol::GeminiGenerateContent {
        return super::claude_gemini::fetch(base_url, api_key, authentication, connection);
    }
    let url = asb_core::endpoint::models_endpoint_for_connection(base_url, protocol, connection)?;
    let client = reqwest::blocking::Client::builder()
        .user_agent("Agent Switchboard")
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法初始化模型列表请求".to_string())?;
    let request = match protocol.resolve_authentication(authentication) {
        asb_core::AuthenticationScheme::Bearer => client.get(&url).bearer_auth(api_key),
        asb_core::AuthenticationScheme::XApiKey => client.get(&url).header("x-api-key", api_key),
        asb_core::AuthenticationScheme::XGoogApiKey => {
            client.get(&url).header("x-goog-api-key", api_key)
        }
    };
    let request = if protocol == UpstreamProtocol::AnthropicMessages {
        request.header("anthropic-version", "2023-06-01")
    } else {
        request
    };
    let mut request = request
        .build()
        .map_err(|_| "无法构造模型列表请求".to_string())?;
    crate::upstream_overrides::apply_header_overrides(request.headers_mut(), connection);
    let secrets = diagnostic_secrets(api_key, connection);
    let secret_refs = secrets.iter().map(String::as_str).collect::<Vec<_>>();
    let response = client
        .execute(request)
        .map_err(|error| network_diagnostic(&url, &error).summary())?;
    if !response.status().is_success() {
        return Err(read_http_diagnostic(response, &secret_refs).summary());
    }
    let mut diagnostic = http_diagnostic(
        &url,
        response.status().as_u16(),
        response.headers(),
        &[],
        false,
        &secret_refs,
    );
    diagnostic.kind = ProviderFailureKind::ResponseParse;
    diagnostic.message = "模型列表响应无法解析".to_string();
    let mut body = Vec::new();
    response
        .take(MAX_MODELS_RESPONSE_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|error| {
            let mut failure = network_diagnostic(&url, &error);
            failure.status = diagnostic.status;
            failure.request_id = diagnostic.request_id.clone();
            failure.summary()
        })?;
    if body.len() as u64 > MAX_MODELS_RESPONSE_BYTES {
        return Err("模型列表响应超过大小限制".into());
    }
    parse_models(&String::from_utf8_lossy(&body)).map_err(|error| {
        diagnostic.message = error;
        diagnostic.summary()
    })
}

/// Builds only the credential headers selected in the provider contract.
/// The Anthropic version header accompanies that protocol alone. The value
/// must never be logged or echoed in diagnostics.
pub(crate) fn provider_auth_headers(
    credential: &str,
    protocol: UpstreamProtocol,
    selected: Option<asb_core::AuthenticationScheme>,
) -> Result<String, String> {
    let google;
    let (credential, selected) = if protocol == UpstreamProtocol::GeminiGenerateContent {
        google = asb_core::claude_gemini::credential(credential, selected)?;
        (google.1.as_str(), Some(google.0))
    } else {
        (credential, selected)
    };
    if credential.chars().any(char::is_control)
        || reqwest::header::HeaderValue::from_str(credential).is_err()
    {
        return Err("API 密钥包含无效的请求头字符".into());
    }
    let authentication = match protocol.resolve_authentication(selected) {
        asb_core::AuthenticationScheme::Bearer => format!("Authorization: Bearer {credential}"),
        asb_core::AuthenticationScheme::XApiKey => format!("x-api-key: {credential}"),
        asb_core::AuthenticationScheme::XGoogApiKey => format!("x-goog-api-key: {credential}"),
    };
    Ok(match protocol {
        UpstreamProtocol::AnthropicMessages => {
            format!("{authentication}\r\nanthropic-version: 2023-06-01")
        }
        UpstreamProtocol::Responses
        | UpstreamProtocol::ChatCompletions
        | UpstreamProtocol::GeminiGenerateContent => authentication,
    })
}

/// Reads only the explicit OpenAI-style `input_modalities` model fact. An
/// absent or malformed field means the source did not establish support; it
/// must not be guessed from a model id or vendor name.
pub(crate) fn image_input_from_entry(entry: &serde_json::Value) -> Option<bool> {
    let modalities = entry.get("input_modalities")?.as_array()?;
    Some(modalities.iter().any(|value| value.as_str() == Some("image")))
}

/// Extracts `data[].id`, optional `data[].owned_by`, and an explicit
/// `data[].input_modalities` fact from an OpenAI-compatible models response.
/// A missing, non-string, or blank `owned_by` stays `None` so the picker groups
/// it under "其他".
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
                    image_input: image_input_from_entry(entry),
                });
            }
        }
    }
    if models.is_empty() {
        return Err("模型列表为空".to_string());
    }
    Ok(models)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_redaction_covers_applied_override_values() {
        let connection = asb_core::contracts::ProviderConnectionOptions {
            custom_user_agent: Some("agent-secret".into()),
            local_proxy_request_overrides: Some(
                asb_core::contracts::LocalProxyRequestOverrides {
                    headers: std::collections::BTreeMap::from([(
                        "x-provider-token".into(),
                        "override-secret".into(),
                    )]),
                    body: serde_json::Value::Null,
                },
            ),
            ..Default::default()
        };
        assert_eq!(
            diagnostic_secrets("api-secret", &connection),
            vec![
                "agent-secret".to_string(),
                "api-secret".to_string(),
                "override-secret".to_string(),
            ],
        );
    }

    #[test]
    fn parses_explicit_image_input_without_guessing_missing_facts() {
        let models = parse_models(
            r#"{"data":[
                {"id":"vision","input_modalities":["text","image"]},
                {"id":"text","input_modalities":["text"]},
                {"id":"unknown"}
            ]}"#,
        )
        .unwrap();

        assert_eq!(models[0].image_input, Some(true));
        assert_eq!(models[1].image_input, Some(false));
        assert_eq!(models[2].image_input, None);
    }
}
