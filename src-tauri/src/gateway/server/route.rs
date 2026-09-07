//! Request-boundary helpers: path-to-protocol routing, client capability
//! token extraction, header passthrough, and upstream URL/header assembly.

use super::super::ActiveRoute;
use asb_core::contracts::UpstreamProtocol;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use tiny_http::Request;

pub(crate) fn upstream_headers(
    route: &ActiveRoute,
    anthropic_version: Option<&str>,
    anthropic_beta: Option<&str>,
) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    match route.upstream_protocol.authentication_scheme() {
        asb_core::AuthenticationScheme::Bearer => {
            if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", route.api_key)) {
                headers.insert(AUTHORIZATION, value);
            }
        }
        asb_core::AuthenticationScheme::XApiKey => {
            if let Ok(value) = HeaderValue::from_str(&route.api_key) {
                headers.insert(HeaderName::from_static("x-api-key"), value);
            }
        }
    }
    if route.upstream_protocol == UpstreamProtocol::AnthropicMessages {
        let version = anthropic_version.unwrap_or("2023-06-01");
        if let Ok(value) = HeaderValue::from_str(version) {
            headers.insert(HeaderName::from_static("anthropic-version"), value);
        }
        if let Some(beta) = anthropic_beta.and_then(|value| HeaderValue::from_str(value).ok()) {
            headers.insert(HeaderName::from_static("anthropic-beta"), beta);
        }
    }
    headers
}

pub(crate) fn upstream_url(route: &ActiveRoute, gateway_base: &str) -> Result<String, String> {
    let base = route.upstream_base_url.trim_end_matches('/');
    let path = match route.upstream_protocol {
        UpstreamProtocol::Responses => "/v1/responses",
        UpstreamProtocol::ChatCompletions => "/v1/chat/completions",
        UpstreamProtocol::AnthropicMessages => "/v1/messages",
    };
    let url = if base.ends_with(path) {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}{}", path.trim_start_matches("/v1"))
    } else {
        format!("{base}{path}")
    };
    let parsed =
        reqwest::Url::parse(&url).map_err(|_| "供应商服务地址无法形成有效上游请求".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || url.starts_with(gateway_base) {
        return Err("供应商服务地址不能指向本机协议网关自身".to_string());
    }
    Ok(url)
}

pub(crate) fn client_protocol(path: &str) -> Option<UpstreamProtocol> {
    let path = path.split('?').next().unwrap_or(path);
    match path {
        "/v1/responses" | "/responses" => Some(UpstreamProtocol::Responses),
        "/v1/messages" | "/messages" => Some(UpstreamProtocol::AnthropicMessages),
        _ => None,
    }
}

pub(crate) fn client_token(request: &Request) -> Option<String> {
    let mut token = None;
    for header in request.headers() {
        let candidate = if header.field.equiv("Authorization") {
            header
                .value
                .as_str()
                .strip_prefix("Bearer ")
                .or_else(|| header.value.as_str().strip_prefix("bearer "))
        } else {
            None
        };
        if let Some(candidate) = candidate {
            if candidate.is_empty() || token.replace(candidate.to_string()).is_some() {
                return None;
            }
        }
    }
    token
}

pub(super) fn request_header(request: &Request, name: &'static str) -> Option<String> {
    let mut value = None;
    for header in request.headers() {
        if header.field.equiv(name) {
            if value.replace(header.value.as_str().to_string()).is_some() {
                return None;
            }
        }
    }
    value
}

pub(super) fn json_content_type(request: &Request) -> bool {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Content-Type"))
        .is_none_or(|header| {
            header
                .value
                .as_str()
                .to_ascii_lowercase()
                .starts_with("application/json")
        })
}
