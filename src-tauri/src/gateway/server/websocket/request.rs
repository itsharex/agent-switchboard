use super::*;

#[cfg(test)]
mod tests;

pub(super) struct RoutedMessage {
    pub route: ActiveRoute,
    pub candidates: Vec<ActiveRoute>,
    pub prepared: PreparedResponse,
}

pub(super) fn prepare(
    inner: &GatewayInner,
    route: ActiveRoute,
    context: &mut ConversationContext,
    text: &str,
) -> Result<RoutedMessage, (String, String)> {
    let candidates = inner.candidates_for(AppKind::Codex, &route);
    let (route, candidates, body) = super::super::codex_forward::resolve_request_route(
        inner,
        route,
        candidates,
        CodexOperation::Responses,
        text.as_bytes().to_vec(),
    )
    .map_err(|(_, message)| ("invalid_request".to_string(), message))?;
    let text = std::str::from_utf8(&body)
        .map_err(|_| ("invalid_request".to_string(), "WebSocket 请求不是 UTF-8".into()))?;
    let mode = route.responses_options
        .map_or(ResponsesRequestMode::Standard, |options| options.request_mode);
    let prepared = if route.upstream_protocol == UpstreamProtocol::Responses
        || crate::gateway::compaction::is_v2(&body).unwrap_or(false)
    {
        context.prepare_native(&route.fingerprint, text)
    } else {
        context.prepare(&route.fingerprint, mode, text)
    }
    .map_err(|error| (error.code.to_string(), error.message))?;
    Ok(RoutedMessage { route, candidates, prepared })
}
