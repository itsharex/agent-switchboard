//! Codex can send Responses over WebSocket. This boundary terminates that
//! local transport, keeps its per-connection replay state private, and sends
//! only a normalized Responses request to the selected upstream protocol.

mod connections;
mod context;

use self::context::{ConversationContext, PendingRequest, PreparedResponse};
use super::super::metrics::RequestSpan;
use super::super::transform::{
    convert_request, convert_response, ReasoningTransport, SseTranscoder,
};
use super::*;
use asb_core::contracts::ResponsesRequestMode;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use reqwest::header::CONTENT_TYPE;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::io::{Read, Write};
use std::sync::atomic::Ordering;
use tiny_http::{Header, Response, StatusCode};
use tungstenite::protocol::{Message, Role, WebSocket, WebSocketConfig};

const WEBSOCKET_ACCEPT_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub(crate) fn handle(
    request: Request,
    inner: Arc<GatewayInner>,
    client: Arc<UpstreamClient>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) {
    if codex_request(request.url()).is_none_or(|path| !path.operation.is_responses()) {
        let endpoint = format!("{}{}", inner.configured_base_url(), request.url());
        let mut diagnostic = ProviderDiagnostic::new(
            ProviderFailureKind::WebsocketUnsupported,
            &endpoint,
            "本机协议网关不支持该 WebSocket endpoint",
        );
        diagnostic.transport = "websocket";
        diagnostic.status = Some(404);
        respond_diagnostic(request, UpstreamProtocol::Responses, 404, &diagnostic);
        return;
    }
    let mut span = RequestSpan::start(
        Arc::clone(&inner.metrics),
        AppKind::Codex,
        UpstreamProtocol::Responses,
    );
    let key = match websocket_key(&request) {
        Ok(key) => key,
        Err(message) => {
            span.finish(Some(400), 0);
            respond_error(request, Some(UpstreamProtocol::Responses), 400, message);
            return;
        }
    };
    let Some((token, route)) = request_capability(&request, UpstreamProtocol::Responses)
        .and_then(|token| active_route(&inner, &token, None).map(|route| (token, route)))
    else {
        span.finish(Some(403), 0);
        respond_error(
            request,
            Some(UpstreamProtocol::Responses),
            403,
            "本机协议网关凭据无效",
        );
        return;
    };
    span.bind_route(
        &route.profile_id,
        &route.fingerprint,
        route.upstream_protocol,
    );
    let Some(_connection) = connections::ConnectionGuard::acquire(Arc::clone(&inner)) else {
        span.finish(Some(503), 0);
        respond_error(
            request,
            Some(UpstreamProtocol::Responses),
            503,
            "本机 WebSocket 连接数已满",
        );
        return;
    };
    let response = Response::empty(StatusCode(101)).with_header(
        Header::from_bytes(
            &b"Sec-WebSocket-Accept"[..],
            websocket_accept(&key).as_bytes(),
        )
        .expect("valid WebSocket accept header"),
    );
    let request_url = request.url().to_string();
    // The HTTP request is consumed by the upgrade. Keep the handshake facts
    // that a native Responses upstream may need for this connection.
    let request_headers = request.headers().to_vec();
    let route_revision = route.fingerprint.clone();
    let stream = match request.upgrade("websocket", response) {
        Ok(stream) => stream,
        Err(_) => {
            span.finish(None, 0);
            log::debug!("本机 WebSocket 升级失败");
            return;
        }
    };
    let config = WebSocketConfig {
        write_buffer_size: 0,
        max_write_buffer_size: MAX_REQUEST_BYTES as usize + 1,
        max_message_size: Some(MAX_REQUEST_BYTES as usize),
        max_frame_size: Some(MAX_REQUEST_BYTES as usize),
        ..WebSocketConfig::default()
    };
    let socket = WebSocket::from_raw_socket(stream, Role::Server, Some(config));
    span.finish(Some(101), 0);
    serve_connection(
        socket,
        inner,
        client,
        token,
        route_revision,
        request_url,
        request_headers,
        stop,
    );
}

fn serve_connection<S>(
    mut socket: WebSocket<S>,
    inner: Arc<GatewayInner>,
    client: Arc<UpstreamClient>,
    token: String,
    route_revision: String,
    request_url: String,
    request_headers: Vec<Header>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) where
    S: Read + Write,
{
    let mut context = ConversationContext::default();
    while !inner.stopping.load(Ordering::Acquire) && !stop.load(Ordering::Acquire) {
        let message = match socket.read() {
            Ok(message) => message,
            Err(_) => break,
        };
        match message {
            Message::Text(text) => {
                let Some(_guard) = crate::gateway::InflightGuard::try_acquire(Arc::clone(&inner))
                else {
                    let _ = send_failed(
                        &mut socket,
                        "gateway_unavailable",
                        "网关正在切换或请求已满；请稍后重试",
                    );
                    break;
                };
                let mut span = RequestSpan::start(
                    Arc::clone(&inner.metrics),
                    AppKind::Codex,
                    UpstreamProtocol::Responses,
                );
                let outcome = serve_message(
                    &mut socket,
                    &client,
                    &inner,
                    &token,
                    &route_revision,
                    &request_url,
                    &request_headers,
                    &mut context,
                    &text,
                    &mut span,
                );
                span.finish(outcome.completed_status(), 0);
                if outcome == ExchangeOutcome::Disconnect {
                    break;
                }
            }
            Message::Ping(_) | Message::Pong(_) => {
                if socket.flush().is_err() {
                    break;
                }
            }
            Message::Close(_) => {
                let _ = socket.flush();
                break;
            }
            Message::Binary(_) | Message::Frame(_) => {
                if !send_failed(
                    &mut socket,
                    "invalid_request",
                    "Codex WebSocket 请求必须是文本消息",
                ) {
                    break;
                }
            }
        }
    }
    let _ = socket.close(None);
}

/// What happened to one WebSocket message exchange. `Served` delivered a
/// complete answer, `Rejected` delivered a failure frame and keeps the
/// connection, `Disconnect` means the socket died around the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExchangeOutcome {
    Served,
    Rejected,
    Disconnect,
}

impl ExchangeOutcome {
    /// The HTTP-style status for telemetry: `None` marks every exchange that
    /// did not deliver a complete converted answer.
    fn completed_status(self) -> Option<u16> {
        match self {
            Self::Served => Some(200),
            Self::Rejected | Self::Disconnect => None,
        }
    }
}

fn serve_message<S>(
    socket: &mut WebSocket<S>,
    client: &UpstreamClient,
    inner: &GatewayInner,
    token: &str,
    route_revision: &str,
    request_url: &str,
    request_headers: &[Header],
    context: &mut ConversationContext,
    text: &str,
    span: &mut RequestSpan,
) -> ExchangeOutcome
where
    S: Read + Write,
{
    if text.len() as u64 > MAX_REQUEST_BYTES {
        return if send_failed(socket, "request_too_large", "请求体超过本机协议网关限制")
        {
            ExchangeOutcome::Rejected
        } else {
            ExchangeOutcome::Disconnect
        };
    }
    let Some(route) = active_route(inner, token, Some(route_revision)) else {
        let _ = send_failed(
            socket,
            "route_inactive",
            "本机协议网关路由已失效；请重新连接后重试",
        );
        return ExchangeOutcome::Disconnect;
    };
    span.bind_route(
        &route.profile_id,
        &route.fingerprint,
        route.upstream_protocol,
    );
    span.note_request_bytes(text.len() as u64);
    let mode = route
        .responses_options
        .map_or(ResponsesRequestMode::Standard, |options| {
            options.request_mode
        });
    let prepared = if route.upstream_protocol == UpstreamProtocol::Responses
        || crate::gateway::compaction::is_v2(text.as_bytes()).unwrap_or(false)
    {
        context.prepare_native(&route.fingerprint, text)
    } else {
        context.prepare(&route.fingerprint, mode, text)
    };
    match prepared {
        Ok(PreparedResponse::Prewarm(response)) => {
            if send_created_and_completed(socket, &response.value) {
                ExchangeOutcome::Served
            } else {
                ExchangeOutcome::Disconnect
            }
        }
        Ok(PreparedResponse::Upstream(mut request)) => {
            match super::codex::resolve_model_and_validate(
                &route,
                CodexOperation::Responses,
                request.body,
            ) {
                Ok(body) => request.body = body,
                Err(message) => {
                    let endpoint = upstream_url(&route, &inner.configured_base_url(), request_url)
                        .unwrap_or_else(|_| route.upstream_base_url.clone());
                    let diagnostic = ProviderDiagnostic::new(
                        ProviderFailureKind::RequestParameters,
                        &endpoint,
                        &message,
                    );
                    return reject_context(socket, &diagnostic, "invalid_request");
                }
            }
            execute_request(
                socket,
                client,
                &inner.configured_base_url(),
                &route,
                context,
                request,
                request_url,
                request_headers,
            )
        }
        Err(error) => {
            let endpoint = upstream_url(&route, &inner.configured_base_url(), request_url)
                .unwrap_or_else(|_| route.upstream_base_url.clone());
            let diagnostic = ProviderDiagnostic::new(
                ProviderFailureKind::RequestParameters,
                &endpoint,
                &error.message,
            );
            reject_context(socket, &diagnostic, error.code)
        }
    }
}
fn active_route(
    inner: &GatewayInner,
    token: &str,
    route_revision: Option<&str>,
) -> Option<ActiveRoute> {
    inner
        .routes
        .read()
        .ok()?
        .get(&AppKind::Codex)
        .filter(|route| constant_time_equal(route.client_token.as_bytes(), token.as_bytes()))
        .filter(|route| route_revision.is_none_or(|revision| route.fingerprint == revision))
        .cloned()
}

mod decode;
mod exchange;
mod handshake;
mod send;
mod stream;

use decode::*;
use exchange::*;
use handshake::*;
use send::*;
