//! Model discovery for Claude managed upstreams. The token never crosses the command boundary.

use super::{http::json_request, request::headers, ResolvedAccount};
use crate::probe::{image_input_from_entry, ProviderModel};
use asb_core::{claude_auth::ClaudeAuthProvider, UpstreamProtocol};
use serde_json::Value;

pub(crate) fn fetch(
    account: &ResolvedAccount,
    protocol: UpstreamProtocol,
) -> Result<Vec<ProviderModel>, String> {
    fetch_for_provider(account, protocol, &Default::default())
}

pub(crate) fn fetch_for_provider(
    account: &ResolvedAccount,
    protocol: UpstreamProtocol,
    connection: &asb_core::contracts::ProviderConnectionOptions,
) -> Result<Vec<ProviderModel>, String> {
    account.provider.validate_protocol(protocol)?;
    let mut url = format!("{}/models", account.endpoint.trim_end_matches('/'));
    if account.provider == ClaudeAuthProvider::CodexOauth {
        url.push_str("?client_version=0.152.1");
    }
    let mut header_map = reqwest::header::HeaderMap::new();
    headers(account, &mut header_map);
    crate::upstream_overrides::apply_header_overrides(&mut header_map, connection);
    let pairs = header_map
        .iter()
        .filter_map(|(name, value)| value.to_str().ok().map(|value| (name.as_str(), value)))
        .collect::<Vec<_>>();
    let response = json_request(reqwest::Method::GET, &url, &pairs, Vec::new())?;
    parse(&response)
}

fn parse(response: &Value) -> Result<Vec<ProviderModel>, String> {
    let entries = response
        .get("data")
        .or_else(|| response.get("models"))
        .and_then(Value::as_array)
        .ok_or("Claude 托管模型列表缺少 data/models 数组")?;
    let mut models = Vec::<ProviderModel>::new();
    for entry in entries {
        let id = entry
            .get("id")
            .or_else(|| entry.get("slug"))
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty() && !id.chars().any(char::is_control));
        if let Some(id) = id {
            if !models.iter().any(|model| model.id == id) {
                models.push(ProviderModel {
                    id: id.into(),
                    owned_by: entry
                        .get("owned_by")
                        .or_else(|| entry.get("vendor"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    image_input: image_input_from_entry(entry),
                });
            }
        }
    }
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn discovery_accepts_chatgpt_slugs_and_deduplicates_without_inventing_models() {
        let models =
            parse(&json!({"models":[{"slug":"gpt-test"},{"slug":"gpt-test"},{"slug":""}]}))
                .unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-test");
        assert!(parse(&json!({"not_models":[]})).is_err());
    }
}
