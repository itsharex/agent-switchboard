//! Upstream HTTP exchange for the local Responses WebSocket boundary.

use super::super::diagnostics::{embedded_error, response_diagnostic};
use super::super::respond::redact_native_body;
use super::*;
use crate::provider_diagnostics::redact_text;

pub(super) fn execute_request<S: Read + Write>(
    socket: &mut WebSocket<S>,
    client: &UpstreamClient,
    gateway_base: &str,
    route: &ActiveRoute,
    context: &mut ConversationContext,
    request: PendingRequest,
    request_url: &str,
    request_headers: &[Header],
    span: &mut RequestSpan,
) -> ExchangeOutcome {
    if route.upstream_protocol != UpstreamProtocol::Responses
        && crate::gateway::compaction::is_v2(&request.body).unwrap_or(false)
    {
        return compact(
            socket,
            client,
            gateway_base,
            route,
            context,
            &request,
            request_url,
            request_headers,
            span,
        );
    }
    let upstream = match request_upstream(
        client,
        gateway_base,
        route,
        &request,
        request_url,
        request_headers,
        span,
    ) {
        Ok(response) => response,
        Err(diagnostic) => return reject_diagnostic(socket, &diagnostic),
    };
    if request.stream {
        relay_stream(socket, upstream, route, context, &request, span)
    } else {
        relay_response(socket, upstream, route, context, &request, span)
    }
}

fn request_upstream(
    client: &UpstreamClient,
    gateway_base: &str,
    route: &ActiveRoute,
    request: &PendingRequest,
    request_url: &str,
    request_headers: &[Header],
    span: &mut RequestSpan,
) -> Result<super::super::UpstreamResponse, ProviderDiagnostic> {
    let url = upstream_url(route, gateway_base, request_url).map_err(|message| {
        ProviderDiagnostic::new(
            ProviderFailureKind::Endpoint,
            &route.upstream_base_url,
            &message,
        )
    })?;
    let (converted, beta) = prepare_body(route, request, &url, request_headers)?;
    span.note_mapped_model(crate::gateway::usage_metadata::model_from_bytes(
        route.upstream_protocol,
        &converted.body,
    ));
    let upstream = super::super::send_upstream_request(
        client,
        route,
        &url,
        reqwest::Method::POST,
        converted.body,
        Some(request_headers),
        None,
        beta.as_deref(),
    )
    .inspect_err(|diagnostic| span.note_codex_attempt(route, diagnostic.status, false))?;
    span.note_codex_attempt(route, Some(upstream.status().as_u16()), false);
    if upstream.initial_body_received() {
        span.note_first_byte();
    }
    if !upstream.status().is_success() {
        return Err(read_upstream_diagnostic(
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

fn prepare_body(
    route: &ActiveRoute,
    request: &PendingRequest,
    url: &str,
    incoming: &[Header],
) -> Result<(ConvertedRequest, Option<String>), ProviderDiagnostic> {
    let invalid = |message: &str| {
        ProviderDiagnostic::new(ProviderFailureKind::RequestParameters, url, message)
    };
    if request.body.len() as u64 > MAX_REQUEST_BYTES {
        return Err(invalid("请求体超过本机协议网关限制"));
    }
    let reasoning_transport = ReasoningTransport::from_continuation_key(route.continuation_key);
    let mut converted = convert_request(
        UpstreamProtocol::Responses,
        route.upstream_protocol,
        &request.body,
        route.max_output_tokens,
        Some(&reasoning_transport),
        route
            .codex
            .as_ref()
            .map(|snapshot| &snapshot.capabilities.chat_reasoning),
    )
    .and_then(|request| crate::gateway::transform::minimal::apply(request, route.responses_options))
    .map_err(|error| invalid(&format!("无法转换 Codex WebSocket 请求：{error}")))?;
    let beta = crate::gateway::codex::request::prepare(
        route,
        &request.body,
        &mut converted.body,
        Some(incoming),
    )
    .map_err(|error| invalid(&error))?;

    if converted.body.len() as u64 > MAX_REQUEST_BYTES {
        return Err(invalid("转换后的请求体超过本机协议网关限制"));
    }
    if converted.stream != request.stream {
        return Err(invalid("转换后的请求流状态与 Codex WebSocket 请求不一致"));
    }
    Ok((converted, beta))
}

fn relay_response<S: Read + Write>(
    socket: &mut WebSocket<S>,
    mut upstream: super::super::UpstreamResponse,
    route: &ActiveRoute,
    context: &mut ConversationContext,
    request: &PendingRequest,
    span: &mut RequestSpan,
) -> ExchangeOutcome {
    let mut diagnostic = response_diagnostic(
        &upstream,
        ProviderFailureKind::ResponseParse,
        "无法转换上游响应",
        &[&route.api_key, &route.client_token],
    );
    let (_, body) = match super::super::respond::read_decoded(&mut upstream) {
        Ok(body) => body,
        Err((kind, message)) => {
            diagnostic.kind = kind;
            diagnostic.message = message;
            return reject_diagnostic(socket, &diagnostic);
        }
    };
    if let Ok(value) = serde_json::from_slice::<Value>(&body) {
        span.note_response_value(route.upstream_protocol, &value);
    }
    let native = route.upstream_protocol == UpstreamProtocol::Responses;
    if !native {
        if let Some(error) =
            embedded_error(&body, &diagnostic, &[&route.api_key, &route.client_token])
        {
            return reject_diagnostic(socket, &error);
        }
    }
    let body = if native {
        match redact_native_body(&body, &[&route.api_key, &route.client_token]) {
            Ok(body) => body,
            Err(message) => {
                diagnostic.message = message;
                return reject_diagnostic(socket, &diagnostic);
            }
        }
    } else {
        body
    };
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
    if native && response_is_failure(&response) {
        let event = match response.get("status").and_then(Value::as_str) {
            Some("incomplete") => "response.incomplete",
            _ => "response.failed",
        };
        return if send_value(socket, &json!({"type": event, "response": response})) {
            ExchangeOutcome::Rejected
        } else {
            ExchangeOutcome::Disconnect
        };
    }
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

fn response_is_failure(response: &Value) -> bool {
    response
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| matches!(status, "failed" | "incomplete"))
        || response.get("error").is_some_and(|error| !error.is_null())
}

fn relay_stream<S: Read + Write>(
    socket: &mut WebSocket<S>,
    upstream: super::super::UpstreamResponse,
    route: &ActiveRoute,
    context: &mut ConversationContext,
    request: &PendingRequest,
    span: &mut RequestSpan,
) -> ExchangeOutcome {
    let mut diagnostic = response_diagnostic(
        &upstream,
        ProviderFailureKind::StreamParse,
        "无法转换上游 SSE 流",
        &[&route.api_key, &route.client_token],
    );
    let reasoning_transport = ReasoningTransport::from_continuation_key(route.continuation_key);
    let headers = upstream.headers().clone();
    let source = match crate::gateway::content_encoding::decode_response_stream(
        &headers,
        upstream,
        MAX_RESPONSE_BYTES as usize,
    ) {
        Ok(source) => source,
        Err(error) => {
            diagnostic.message = format!("无法解压上游 SSE：{error}");
            return reject_diagnostic(socket, &diagnostic);
        }
    };
    let stream = match SseTranscoder::new(
        source,
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
    super::stream::send_stream(socket, stream, route, context, request, diagnostic, span)
}

fn compact<S: Read + Write>(
    socket: &mut WebSocket<S>,
    client: &UpstreamClient,
    gateway_base: &str,
    route: &ActiveRoute,
    context: &mut ConversationContext,
    request: &PendingRequest,
    request_url: &str,
    request_headers: &[Header],
    span: &mut RequestSpan,
) -> ExchangeOutcome {
    let result = match crate::gateway::compaction::execute(
        client,
        route,
        gateway_base,
        request_url,
        &request.body,
        Some(request_headers),
    ) {
        Ok(result) => result,
        Err(diagnostic) => {
            span.note_codex_attempt(route, diagnostic.status, false);
            return reject_diagnostic(socket, &diagnostic);
        }
    };
    span.note_codex_attempt(route, Some(200), false);
    span.note_mapped_model(result.upstream_model.clone());
    span.note_response_value(UpstreamProtocol::Responses, &result.response);
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
