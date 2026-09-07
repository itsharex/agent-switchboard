//! Codex's Responses transport is a WebSocket. This boundary terminates that
//! local transport, keeps its per-connection replay state private, and sends
//! only a normalized Responses request to the selected upstream protocol.

mod context;

use self::context::{ConversationContext, PendingRequest, PreparedResponse};
use super::super::metrics::RequestSpan;
use super::super::transform::{
    convert_request, convert_response, ReasoningTransport, SseTranscoder,
};
use super::*;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use reqwest::blocking::Response as UpstreamResponse;
use reqwest::header::CONTENT_TYPE;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::io::{Read, Write};
use tiny_http::{Header, Response, StatusCode};
use tungstenite::protocol::{Message, Role, WebSocket, WebSocketConfig};

const WEBSOCKET_ACCEPT_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub(super) fn is_upgrade_request(request: &Request) -> bool {
    request.method() == &Method::Get
        && request.headers().iter().any(|header| {
            header.field.equiv("Upgrade") && header_contains(header.value.as_str(), "websocket")
        })
}

pub(super) fn handle(request: Request, inner: Arc<GatewayInner>, client: Arc<Client>) {
    if client_protocol(request.url()) != Some(UpstreamProtocol::Responses) {
        respond_error(request, None, 404, "本机协议网关不处理该 WebSocket 路径");
        return;
    }
    let span = RequestSpan::start(
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
    let Some(token) = client_token(&request) else {
        span.finish(Some(401), 0);
        respond_error(
            request,
            Some(UpstreamProtocol::Responses),
            401,
            "本机协议网关凭据无效",
        );
        return;
    };
    if active_route(&inner, &token).is_none() {
        span.finish(Some(401), 0);
        respond_error(
            request,
            Some(UpstreamProtocol::Responses),
            401,
            "本机协议网关凭据无效",
        );
        return;
    }
    let response = Response::empty(StatusCode(101)).with_header(
        Header::from_bytes(
            &b"Sec-WebSocket-Accept"[..],
            websocket_accept(&key).as_bytes(),
        )
        .expect("valid WebSocket accept header"),
    );
    let stream = request.upgrade("websocket", response);
    let config = WebSocketConfig {
        write_buffer_size: 0,
        max_write_buffer_size: MAX_REQUEST_BYTES as usize + 1,
        max_message_size: Some(MAX_REQUEST_BYTES as usize),
        max_frame_size: Some(MAX_REQUEST_BYTES as usize),
        ..WebSocketConfig::default()
    };
    let socket = WebSocket::from_raw_socket(stream, Role::Server, Some(config));
    serve_connection(socket, inner, client, token);
}

fn serve_connection<S>(
    mut socket: WebSocket<S>,
    inner: Arc<GatewayInner>,
    client: Arc<Client>,
    token: String,
) where
    S: Read + Write,
{
    let mut context = ConversationContext::default();
    while !inner.stopping.load(Ordering::Acquire) {
        let message = match socket.read() {
            Ok(message) => message,
            Err(_) => break,
        };
        match message {
            Message::Text(text) => {
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
    client: &Client,
    inner: &GatewayInner,
    token: &str,
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
    let Some(route) = active_route(inner, token) else {
        return if send_failed(
            socket,
            "route_inactive",
            "本机协议网关路由已失效；请重新连接后重试",
        ) {
            ExchangeOutcome::Rejected
        } else {
            ExchangeOutcome::Disconnect
        };
    };
    span.bind_route(&route.profile_id, route.upstream_protocol);
    span.note_request_bytes(text.len() as u64);
    match context.prepare(text) {
        Ok(PreparedResponse::Prewarm(response)) => {
            if send_created_and_completed(socket, &response.value) {
                ExchangeOutcome::Served
            } else {
                ExchangeOutcome::Disconnect
            }
        }
        Ok(PreparedResponse::Upstream(request)) => {
            if execute_request(socket, client, &inner.base_url, &route, context, request) {
                ExchangeOutcome::Served
            } else {
                ExchangeOutcome::Disconnect
            }
        }
        Err(error) => {
            if send_failed(socket, error.code, &error.message) {
                ExchangeOutcome::Rejected
            } else {
                ExchangeOutcome::Disconnect
            }
        }
    }
}
fn active_route(inner: &GatewayInner, token: &str) -> Option<ActiveRoute> {
    inner
        .routes
        .read()
        .ok()?
        .get(&AppKind::Codex)
        .filter(|route| constant_time_equal(route.client_token.as_bytes(), token.as_bytes()))
        .cloned()
}

mod decode;
mod exchange;
mod handshake;
mod send;

use decode::*;
use exchange::*;
use handshake::*;
use send::*;
