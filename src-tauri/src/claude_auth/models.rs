//! Model discovery for Claude managed upstreams. The token never crosses the command boundary.
//! Catalog parsing is owned by [`crate::probe::parse_models_value`]; managed
//! upstreams (Anthropic, ChatGPT backend, Copilot, xAI) all serve one of its
//! documented wire shapes.

use super::{http::json_request, request::headers, ResolvedAccount};
use crate::probe::{parse_models_value, ProviderModel};
use asb_core::{claude_auth::ClaudeAuthProvider, UpstreamProtocol};

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
    parse_models_value(&response)
}
