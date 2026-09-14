//! Import query tables as explicit operation URLs so ASB keeps one endpoint owner.
use crate::contracts::{CodexEndpoint, CodexUpstream, ProviderConnectionOptions};
use toml_edit::{DocumentMut, Item};

pub(super) fn apply(
    document: &DocumentMut,
    endpoint: CodexEndpoint,
    upstream: CodexUpstream,
    connection: &mut ProviderConnectionOptions,
) -> Result<CodexEndpoint, String> {
    let Some(provider) = document.get("model_provider").and_then(Item::as_str) else {
        return Ok(endpoint);
    };
    let table = document
        .get("model_providers")
        .and_then(Item::as_table_like)
        .and_then(|providers| providers.get(provider))
        .and_then(Item::as_table_like);
    let Some(query) = table.and_then(|table| table.get("query_params")) else {
        return Ok(endpoint);
    };
    let query = query
        .as_table_like()
        .ok_or("Codex query_params 必须是字符串表")?;
    let mut pairs = Vec::new();
    for (key, value) in query.iter() {
        if key.is_empty() || key.chars().any(char::is_control) {
            return Err("Codex query_params 的参数名无效".into());
        }
        let value = value
            .as_str()
            .ok_or("Codex query_params 的值必须是字符串")?;
        pairs.push((key, value));
    }
    if pairs.is_empty() {
        return Ok(endpoint);
    }
    let operation = |base: &str| -> Result<String, String> {
        let endpoint = crate::endpoint::upstream_endpoint_with_options(
            base,
            upstream.protocol(),
            connection.is_full_url,
        )?;
        let mut url = url::Url::parse(&endpoint).map_err(|_| "Codex 查询参数地址无效")?;
        url.query_pairs_mut().extend_pairs(pairs.iter().copied());
        Ok(url.to_string())
    };
    let endpoint = CodexEndpoint(operation(&endpoint.0)?);
    let mut candidates = std::collections::BTreeMap::new();
    for candidate in connection.custom_endpoints.values() {
        let mut candidate = candidate.clone();
        candidate.url = operation(&candidate.url)?;
        candidates.insert(candidate.url.clone(), candidate);
    }
    connection.custom_endpoints = candidates;
    connection.is_full_url = true;
    Ok(endpoint)
}
