//! Loopback HTTP boundary. It authenticates a client capability before any
//! body is forwarded, keeps upstream credentials local, and emits only
//! gateway-owned diagnostics.

mod codex;
mod compact;
mod diagnostics;
mod respond;
mod route;
mod transport;
pub(crate) mod websocket;

use super::http::Request;
use super::metrics::RequestSpan;
use super::transform::{convert_request, ConvertedRequest, ReasoningTransport};
use super::{constant_time_equal, content_encoding, ActiveRoute, BoundListener, GatewayInner};
use asb_core::contracts::{AppKind, UpstreamProtocol};
#[cfg(test)]
use reqwest::blocking::Client;
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
#[cfg(test)]
use std::time::Duration;
use tiny_http::Method;
#[cfg(test)]
use tiny_http::Server;

use crate::provider_diagnostics::{ProviderDiagnostic, ProviderFailureKind};
use diagnostics::respond_diagnostic;
use route::{json_content_type, request_header};

pub(super) use respond::{
    read_limited, read_upstream_diagnostic, respond_error, respond_upstream, ReadLimitError,
};
pub(super) use route::{
    client_protocol, codex_request, request_capability, upstream_codex_operation_url,
    upstream_compact_url, upstream_url, CodexOperation, UpstreamClient, UpstreamResponse,
};
pub(super) use transport::send_upstream_request;

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
    super::http::serve(listener, inner, ready);
}

pub(crate) fn handle(request: Request, inner: Arc<GatewayInner>, client: Arc<UpstreamClient>) {
    let codex_operation = codex_request(request.url()).map(|path| path.operation);
    let target_protocol = client_protocol(request.url());
    let expected_method = codex_operation
        .map(CodexOperation::method)
        .unwrap_or(Method::Post);
    if request.method() != &expected_method {
        respond_error(
            request,
            target_protocol,
            405,
            "此本机协议网关操作不接受该 HTTP 方法",
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
    if codex_operation.is_none_or(CodexOperation::requires_json) && !json_content_type(&request) {
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
    span.bind_route(
        &route.profile_id,
        &route.fingerprint,
        route.upstream_protocol,
    );
    if let Some(operation) = codex_operation {
        if let Err(error) = codex::ensure_operation(&route, operation) {
            span.finish(Some(501), 0);
            respond_error(request, Some(protocol), 501, &error);
            return;
        }
        if operation == CodexOperation::Models {
            codex::respond_models(request, span, &route);
            return;
        }
    }
    forward_request(
        request,
        span,
        protocol,
        route,
        inner,
        client,
        codex_operation,
    );
}

fn forward_request(
    mut request: Request,
    mut span: RequestSpan,
    protocol: UpstreamProtocol,
    route: ActiveRoute,
    inner: Arc<GatewayInner>,
    client: Arc<UpstreamClient>,
    codex_operation: Option<CodexOperation>,
) {
    let is_compact = codex_operation.is_some_and(CodexOperation::is_compact);
    let url = match if is_compact && route.upstream_protocol == UpstreamProtocol::Responses {
        upstream_compact_url(&route, &inner.configured_base_url(), request.url())
    } else if let Some(operation) =
        codex_operation.filter(|operation| !operation.is_responses() && !operation.is_compact())
    {
        upstream_codex_operation_url(
            &route,
            &inner.configured_base_url(),
            operation,
            request.url(),
        )
    } else {
        upstream_url(&route, &inner.configured_base_url(), request.url())
    } {
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
    let raw_body = request.take_body();
    let body = match content_encoding::decode_request_body(
        request.headers(),
        &raw_body,
        MAX_REQUEST_BYTES as usize,
    ) {
        Ok(body) => body,
        Err(content_encoding::DecodeError::TooLarge) => {
            span.finish(Some(413), 0);
            respond_error(
                request,
                Some(protocol),
                413,
                "解压后的请求体超过本机协议网关限制",
            );
            return;
        }
        Err(content_encoding::DecodeError::Unsupported(coding)) => {
            span.finish(Some(415), 0);
            respond_error(
                request,
                Some(protocol),
                415,
                &format!("不支持请求 Content-Encoding: {coding}"),
            );
            return;
        }
        Err(content_encoding::DecodeError::Invalid(error)) => {
            span.finish(Some(422), 0);
            respond_error(
                request,
                Some(protocol),
                422,
                &format!("无法解压请求体: {error}"),
            );
            return;
        }
    };
    let body = match codex_operation {
        Some(operation) => match codex::resolve_model_and_validate(&route, operation, body) {
            Ok(body) => body,
            Err(error) => {
                span.finish(Some(422), 0);
                respond_error(request, Some(protocol), 422, &error);
                return;
            }
        },
        None => body,
    };
    span.note_request_bytes(body.len() as u64);
    if is_compact && route.upstream_protocol == UpstreamProtocol::Responses {
        let reasoning_transport = ReasoningTransport::from_continuation_key(route.continuation_key);
        let converted = match convert_request(
            UpstreamProtocol::Responses,
            UpstreamProtocol::Responses,
            &body,
            route.max_output_tokens,
            Some(&reasoning_transport),
            None,
        )
        .and_then(|converted| super::transform::minimal::apply(converted, route.responses_options))
        {
            Ok(converted) => converted,
            Err(error) => {
                span.finish(Some(422), 0);
                let diagnostic = ProviderDiagnostic::new(
                    ProviderFailureKind::RequestParameters,
                    &url,
                    &format!("无法准备原生 Responses compact 请求：{}", error.0),
                );
                respond_diagnostic(request, protocol, 422, &diagnostic);
                return;
            }
        };
        send_upstream(request, span, protocol, route, url, client, converted);
        return;
    }
    if let Some(operation) =
        codex_operation.filter(|operation| !operation.is_responses() && !operation.is_compact())
    {
        send_codex_operation(request, span, protocol, route, url, client, operation, body);
        return;
    }
    if protocol == UpstreamProtocol::Responses
        && route.upstream_protocol != UpstreamProtocol::Responses
        && (is_compact || super::compaction::is_v2(&body).unwrap_or(false))
    {
        compact::respond(request, span, &route, &inner, &client, &body, is_compact);
        return;
    }
    let reasoning_transport = ReasoningTransport::from_continuation_key(route.continuation_key);
    let converted = match convert_request(
        protocol,
        route.upstream_protocol,
        &body,
        route.max_output_tokens,
        Some(&reasoning_transport),
        route
            .codex
            .as_ref()
            .map(|snapshot| &snapshot.capabilities.chat_reasoning),
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

fn send_codex_operation(
    request: Request,
    span: RequestSpan,
    protocol: UpstreamProtocol,
    route: ActiveRoute,
    url: String,
    client: Arc<UpstreamClient>,
    operation: CodexOperation,
    body: Vec<u8>,
) {
    let method = match operation.method() {
        Method::Get => reqwest::Method::GET,
        Method::Post => reqwest::Method::POST,
        _ => unreachable!("CodexOperation only exposes GET and POST"),
    };
    let upstream = match send_upstream_request(
        &client,
        &route,
        &url,
        method,
        body,
        Some(request.headers()),
        None,
        None,
    ) {
        Ok(response) => response,
        Err(diagnostic) => {
            span.finish(Some(502), 0);
            respond_diagnostic(request, protocol, 502, &diagnostic);
            return;
        }
    };
    respond::respond_passthrough(request, span, protocol, &route, upstream);
}

fn send_upstream(
    request: Request,
    span: RequestSpan,
    protocol: UpstreamProtocol,
    route: ActiveRoute,
    url: String,
    client: Arc<UpstreamClient>,
    converted: ConvertedRequest,
) {
    let anthropic_version = request_header(&request, "anthropic-version");
    let anthropic_beta = request_header(&request, "anthropic-beta");
    let upstream = match send_upstream_request(
        &client,
        &route,
        &url,
        reqwest::Method::POST,
        converted.body,
        Some(request.headers()),
        anthropic_version.as_deref(),
        anthropic_beta.as_deref(),
    ) {
        Ok(response) => response,
        Err(diagnostic) => {
            span.finish(Some(502), 0);
            respond_diagnostic(request, protocol, 502, &diagnostic);
            return;
        }
    };
    respond_upstream(request, span, protocol, &route, converted.stream, upstream);
}

#[cfg(test)]
mod live_codex_cli_tests;
#[cfg(test)]
mod live_codex_tests;
#[cfg(test)]
mod live_support;
#[cfg(test)]
pub(crate) mod tests;
