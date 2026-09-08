//! Upstream HTTP exchange for the local Responses WebSocket boundary.

use super::super::diagnostics::{embedded_error, response_diagnostic};
use super::*;
use crate::provider_diagnostics::{read_http_diagnostic, redact_text};

pub(super) fn execute_request<S: Read + Write>(
    socket: &mut WebSocket<S>,
    client: &Client,
    gateway_base: &str,
    route: &ActiveRoute,
    context: &mut ConversationContext,
    request: PendingRequest,
) -> ExchangeOutcome {
    if crate::gateway::compaction::is_v2(&request.body).unwrap_or(false) {
        return compact(socket, client, gateway_base, route, context, &request);
    }
    let upstream = match request_upstream(client, gateway_base, route, &request) {
        Ok(response) => response,
        Err(diagnostic) => return reject_diagnostic(socket, &diagnostic),
    };
    if request.stream {
        relay_stream(socket, upstream, route, context, &request)
    } else {
        relay_response(socket, upstream, route, context, &request)
    }
}

fn request_upstream(
    client: &Client,
    gateway_base: &str,
    route: &ActiveRoute,
    request: &PendingRequest,
) -> Result<UpstreamResponse, ProviderDiagnostic> {
    let url = upstream_url(route, gateway_base).map_err(|message| {
        ProviderDiagnostic::new(
            ProviderFailureKind::Endpoint,
            &route.upstream_base_url,
            &message,
        )
    })?;
    let invalid = |message: &str| {
        ProviderDiagnostic::new(ProviderFailureKind::RequestParameters, &url, message)
    };
    if request.body.len() as u64 > MAX_REQUEST_BYTES {
        return Err(invalid("请求体超过本机协议网关限制"));
    }
    let reasoning_transport = ReasoningTransport::from_continuation_key(route.continuation_key);
    let converted = convert_request(
        UpstreamProtocol::Responses,
        route.upstream_protocol,
        &request.body,
        route.max_output_tokens,
        Some(&reasoning_transport),
    )
    .and_then(|request| crate::gateway::transform::minimal::apply(request, route.responses_options))
    .map_err(|error| invalid(&format!("无法转换 Codex WebSocket 请求：{error}")))?;
    if converted.body.len() as u64 > MAX_REQUEST_BYTES {
        return Err(invalid("转换后的请求体超过本机协议网关限制"));
    }
    if converted.stream != request.stream {
        return Err(invalid("转换后的请求流状态与 Codex WebSocket 请求不一致"));
    }
    let upstream = client
        .post(&url)
        .headers(upstream_headers(route, None, None))
        .body(converted.body)
        .send()
        .map_err(|error| network_diagnostic(&url, &error))?;
    if !upstream.status().is_success() {
        return Err(read_http_diagnostic(
            upstream,
            &[&route.api_key, &route.client_token],
        ));
    }
    let upstream_stream = upstream
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("text/event-stream"));
    if request.stream != upstream_stream {
        return Err(response_diagnostic(
            &upstream,
            ProviderFailureKind::StreamParse,
            if request.stream {
                "上游服务未按请求返回 SSE 流"
            } else {
                "上游服务返回了未请求的 SSE 流"
            },
            &[&route.api_key, &route.client_token],
        ));
    }
    Ok(upstream)
}

fn relay_response<S: Read + Write>(
    socket: &mut WebSocket<S>,
    mut upstream: UpstreamResponse,
    route: &ActiveRoute,
    context: &mut ConversationContext,
    request: &PendingRequest,
) -> ExchangeOutcome {
    let mut diagnostic = response_diagnostic(
        &upstream,
        ProviderFailureKind::ResponseParse,
        "无法转换上游响应",
        &[&route.api_key, &route.client_token],
    );
    let body = match read_limited(&mut upstream, MAX_RESPONSE_BYTES) {
        Ok(body) => body,
        Err(error) => {
            diagnostic.message = match error {
                ReadLimitError::TooLarge => "上游响应超过本机协议网关限制",
                ReadLimitError::Io => {
                    diagnostic.kind = ProviderFailureKind::Network;
                    "读取上游响应时连接中断"
                }
            }
            .to_string();
            return reject_diagnostic(socket, &diagnostic);
        }
    };
    if let Some(error) = embedded_error(&body, &diagnostic, &[&route.api_key, &route.client_token])
    {
        return reject_diagnostic(socket, &error);
    }
    let reasoning_transport = ReasoningTransport::from_continuation_key(route.continuation_key);
    let body = match convert_response(
        route.upstream_protocol,
        UpstreamProtocol::Responses,
        &body,
        Some(&reasoning_transport),
    ) {
        Ok(body) => body,
        Err(error) => {
            diagnostic.message = redact_text(
                &format!("无法转换上游响应：{error}"),
                &[&route.api_key, &route.client_token],
            );
            return reject_diagnostic(socket, &diagnostic);
        }
    };
    let response: Value = match serde_json::from_slice(&body) {
        Ok(response) => response,
        Err(error) => {
            diagnostic.message = format!("转换后的响应不是有效 JSON：{error}");
            return reject_diagnostic(socket, &diagnostic);
        }
    };
    if let Err(error) = context.record_completed(request, &response) {
        diagnostic.message = error.message;
        return reject_diagnostic(socket, &diagnostic);
    }
    if send_created_and_completed(socket, &response) {
        ExchangeOutcome::Served
    } else {
        ExchangeOutcome::Disconnect
    }
}

fn relay_stream<S: Read + Write>(
    socket: &mut WebSocket<S>,
    upstream: UpstreamResponse,
    route: &ActiveRoute,
    context: &mut ConversationContext,
    request: &PendingRequest,
) -> ExchangeOutcome {
    let mut diagnostic = response_diagnostic(
        &upstream,
        ProviderFailureKind::StreamParse,
        "无法转换上游 SSE 流",
        &[&route.api_key, &route.client_token],
    );
    let reasoning_transport = ReasoningTransport::from_continuation_key(route.continuation_key);
    let stream = match SseTranscoder::new(
        upstream,
        route.upstream_protocol,
        UpstreamProtocol::Responses,
        MAX_RESPONSE_BYTES,
        Some(&reasoning_transport),
    ) {
        Ok(stream) => {
            stream.with_diagnostics(diagnostic.clone(), &[&route.api_key, &route.client_token])
        }
        Err(error) => {
            diagnostic.message = format!("无法转换上游 SSE 流：{error}");
            return reject_diagnostic(socket, &diagnostic);
        }
    };
    super::stream::send_stream(socket, stream, route, context, request, diagnostic)
}

fn compact<S: Read + Write>(
    socket: &mut WebSocket<S>,
    client: &Client,
    gateway_base: &str,
    route: &ActiveRoute,
    context: &mut ConversationContext,
    request: &PendingRequest,
) -> ExchangeOutcome {
    let result =
        match crate::gateway::compaction::execute(client, route, gateway_base, &request.body) {
            Ok(result) => result,
            Err(diagnostic) => return reject_diagnostic(socket, &diagnostic),
        };
    if let Err(error) = context.record_completed(request, &result.response) {
        return if send_failed(socket, error.code, &error.message) {
            ExchangeOutcome::Rejected
        } else {
            ExchangeOutcome::Disconnect
        };
    }
    for event in result.events() {
        if !send_value(socket, &event) {
            return ExchangeOutcome::Disconnect;
        }
    }
    ExchangeOutcome::Served
}
