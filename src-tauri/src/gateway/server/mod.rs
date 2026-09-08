//! Loopback HTTP boundary. It authenticates a client capability before any
//! body is forwarded, keeps upstream credentials local, and emits only
//! gateway-owned diagnostics.

mod compact;
mod diagnostics;
mod respond;
mod route;
pub(crate) mod websocket;

use super::http::Request;
use super::metrics::RequestSpan;
use super::transform::{convert_request, ConvertedRequest, ReasoningTransport};
use super::{constant_time_equal, ActiveRoute, BoundListener, GatewayInner};
use asb_core::contracts::{AppKind, UpstreamProtocol};
use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::time::Duration;
use tiny_http::Method;
#[cfg(test)]
use tiny_http::Server;

use crate::provider_diagnostics::{network_diagnostic, ProviderDiagnostic, ProviderFailureKind};
use diagnostics::respond_diagnostic;
use route::{json_content_type, request_header};

pub(super) use respond::{read_limited, respond_error, respond_upstream, ReadLimitError};
pub(super) use route::{client_protocol, request_capability, upstream_headers, upstream_url};

pub(super) const MAX_REQUEST_BYTES: u64 = 8 * 1024 * 1024;
pub(super) const MAX_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;

/// The client behind a served path. Chat Completions is upstream-only here,
/// so it has no client and its traffic is not counted as provider traffic.
fn span_app(protocol: UpstreamProtocol) -> Option<AppKind> {
    match protocol {
        UpstreamProtocol::Responses => Some(AppKind::Codex),
        UpstreamProtocol::AnthropicMessages => Some(AppKind::Claude),
        UpstreamProtocol::ChatCompletions => None,
    }
}

pub(super) fn serve(
    listener: BoundListener,
    inner: Arc<GatewayInner>,
    ready: SyncSender<Result<(), String>>,
) {
    let client = match Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .build()
    {
        Ok(client) => Arc::new(client),
        Err(error) => {
            let _ = ready.send(Err(format!("无法初始化网关 HTTP 客户端：{error}")));
            return;
        }
    };
    super::http::serve(listener, inner, client, ready);
}

pub(crate) fn handle(request: Request, inner: Arc<GatewayInner>, client: Arc<Client>) {
    let target_protocol = client_protocol(request.url());
    if request.method() != &Method::Post {
        respond_error(
            request,
            target_protocol,
            405,
            "本机协议网关仅接受 POST 请求",
        );
        return;
    }
    let Some(protocol) = target_protocol else {
        respond_error(request, None, 404, "本机协议网关不处理该路径");
        return;
    };
    // Provider traffic starts at a path naming a client the gateway serves;
    // every rejection from here on is recorded with the span.
    let Some(app) = span_app(protocol) else {
        respond_error(
            request,
            Some(protocol),
            404,
            "本机协议网关不接收 Chat Completions 客户端请求",
        );
        return;
    };
    let mut span = RequestSpan::start(Arc::clone(&inner.metrics), app, protocol);
    if !json_content_type(&request) {
        span.finish(Some(415), 0);
        respond_error(
            request,
            Some(protocol),
            415,
            "请求必须使用 application/json",
        );
        return;
    }
    let Some(token) = request_capability(&request, protocol) else {
        span.finish(Some(403), 0);
        respond_error(request, Some(protocol), 403, "本机协议网关凭据无效");
        return;
    };
    let route = match inner.routes.read() {
        Ok(routes) => routes
            .get(&app)
            .filter(|route| constant_time_equal(route.client_token.as_bytes(), token.as_bytes()))
            .cloned(),
        Err(_) => None,
    };
    let Some(route) = route else {
        span.finish(Some(403), 0);
        respond_error(request, Some(protocol), 403, "本机协议网关凭据无效");
        return;
    };
    span.bind_route(&route.profile_id, route.upstream_protocol);
    forward_request(request, span, protocol, route, inner, client);
}

fn forward_request(
    mut request: Request,
    mut span: RequestSpan,
    protocol: UpstreamProtocol,
    route: ActiveRoute,
    inner: Arc<GatewayInner>,
    client: Arc<Client>,
) {
    let url = match upstream_url(&route, &inner.configured_base_url()) {
        Ok(url) => url,
        Err(message) => {
            span.finish(Some(502), 0);
            let diagnostic = ProviderDiagnostic::new(
                ProviderFailureKind::Endpoint,
                &route.upstream_base_url,
                &message,
            );
            respond_diagnostic(request, protocol, 502, &diagnostic);
            return;
        }
    };
    let body = request.take_body();
    span.note_request_bytes(body.len() as u64);
    let legacy_compact = route::codex_path(request.url()).is_some_and(|(_, compact)| compact);
    if protocol == UpstreamProtocol::Responses
        && (legacy_compact || super::compaction::is_v2(&body).unwrap_or(false))
    {
        compact::respond(
            request,
            span,
            &route,
            &inner,
            &client,
            &body,
            legacy_compact,
        );
        return;
    }
    let reasoning_transport = ReasoningTransport::from_continuation_key(route.continuation_key);
    let converted = match convert_request(
        protocol,
        route.upstream_protocol,
        &body,
        route.max_output_tokens,
        Some(&reasoning_transport),
    )
    .and_then(|converted| super::transform::minimal::apply(converted, route.responses_options))
    {
        Ok(converted) => converted,
        Err(error) => {
            span.finish(Some(422), 0);
            let diagnostic = ProviderDiagnostic::new(
                ProviderFailureKind::RequestParameters,
                &url,
                &format!("无法转换请求：{error}"),
            );
            respond_diagnostic(request, protocol, 422, &diagnostic);
            return;
        }
    };
    if converted.body.len() as u64 > MAX_REQUEST_BYTES {
        span.finish(Some(413), 0);
        respond_error(
            request,
            Some(protocol),
            413,
            "转换后的请求体超过本机协议网关限制",
        );
        return;
    }
    send_upstream(request, span, protocol, route, url, client, converted);
}

fn send_upstream(
    request: Request,
    span: RequestSpan,
    protocol: UpstreamProtocol,
    route: ActiveRoute,
    url: String,
    client: Arc<Client>,
    converted: ConvertedRequest,
) {
    let headers = upstream_headers(
        &route,
        request_header(&request, "anthropic-version").as_deref(),
        request_header(&request, "anthropic-beta").as_deref(),
    );
    let upstream = match client
        .post(&url)
        .headers(headers)
        .body(converted.body)
        .send()
    {
        Ok(response) => response,
        Err(error) => {
            span.finish(Some(502), 0);
            respond_diagnostic(request, protocol, 502, &network_diagnostic(&url, &error));
            return;
        }
    };
    respond_upstream(request, span, protocol, &route, converted.stream, upstream);
}

#[cfg(test)]
mod live_nvidia_tests;
#[cfg(test)]
mod tests;
