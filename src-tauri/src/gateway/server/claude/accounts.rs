//! Resolve a Claude managed upstream for one attempt; never mutate the saved route.

use super::*;
use crate::claude_auth::request as auth_request;
use crate::gateway::server::respond::read_limited;

pub(super) fn resolve(
    candidate: &ActiveRoute,
    inner: &GatewayInner,
) -> Result<ActiveRoute, String> {
    let mut route = candidate.clone();
    if let Some(account) = inner.claude_auth.resolve(&candidate.connection)? {
        account
            .provider
            .validate_protocol(route.upstream_protocol)?;
        route.upstream_base_url = account.endpoint.clone();
        route.connection.is_full_url = false;
        route.api_key = account.access_token.clone();
        route.authentication = asb_core::AuthenticationScheme::Bearer;
        route.continuation_key = auth_request::continuation_key(route.continuation_key, &account);
        route.claude_account = Some(account);
    }
    if route.upstream_protocol == UpstreamProtocol::GeminiGenerateContent {
        let (scheme, key) =
            asb_core::claude_gemini::credential(&route.api_key, Some(route.authentication))?;
        route.api_key = key;
        route.authentication = scheme;
    }
    Ok(route)
}

pub(super) fn prepare_body(
    route: &ActiveRoute,
    body: &mut Vec<u8>,
    original: &[u8],
) -> Result<(), String> {
    if let Some(account) = &route.claude_account {
        if account.provider == asb_core::claude_auth::ClaudeAuthProvider::GithubCopilot {
            crate::claude_auth::copilot_model::apply(body, original)?;
        }
        auth_request::body(account, route.upstream_protocol, body)?;
    }
    Ok(())
}

pub(super) fn non_streaming(
    mut upstream: UpstreamResponse,
    route: &ActiveRoute,
) -> Result<UpstreamResponse, ProviderDiagnostic> {
    let forced = route.claude_account.as_ref().is_some_and(|account| {
        account.provider == asb_core::claude_auth::ClaudeAuthProvider::CodexOauth
    });
    if !forced {
        return Ok(upstream);
    }
    let mut diagnostic = super::super::diagnostics::response_diagnostic(
        &upstream,
        ProviderFailureKind::StreamParse,
        "Claude 托管上游响应无效",
        &[&route.api_key, &route.client_token],
    );
    let encoded = read_limited(&mut upstream, MAX_RESPONSE_BYTES).map_err(|_| {
        diagnostic.message = "Claude 托管上游 SSE 读取失败或超过大小限制".into();
        diagnostic.clone()
    })?;
    let decoded = content_encoding::decode_response_body(
        upstream.headers(),
        &encoded,
        MAX_RESPONSE_BYTES as usize,
    )
    .map_err(|error| {
        diagnostic.message = error.to_string();
        diagnostic.clone()
    })?;
    let body = auth_request::completed_response(&decoded).map_err(|message| {
        diagnostic.message = message;
        diagnostic.clone()
    })?;
    Ok(upstream.with_json_body(body))
}
