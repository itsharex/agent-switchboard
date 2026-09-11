//! Upstream response handling: converted JSON bodies, chunked SSE relaying,
//! error rendering, and the shared size-limited reader.

use super::super::metrics::RequestSpan;
use super::super::transform::{convert_error, convert_response, ReasoningTransport, SseTranscoder};
use super::super::{content_encoding, ActiveRoute};
use super::diagnostics::{embedded_error, respond_diagnostic, response_diagnostic};
use super::UpstreamResponse;
use super::MAX_RESPONSE_BYTES;
use crate::gateway::http::Request;
use crate::provider_diagnostics::{
    http_diagnostic, ProviderDiagnostic, ProviderFailureKind, MAX_DIAGNOSTIC_BODY_BYTES,
};
use asb_core::contracts::UpstreamProtocol;
use reqwest::header::{HeaderMap, CONTENT_TYPE};
use std::io::{Cursor, Read, Write};
use tiny_http::{Header, Response, StatusCode};

pub(crate) fn respond_upstream(
    request: Request,
    span: RequestSpan,
    client_protocol: UpstreamProtocol,
    route: &ActiveRoute,
    requested_stream: bool,
    upstream: UpstreamResponse,
) {
    let status = upstream.status().as_u16();
    let upstream_stream = upstream
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("text/event-stream"));
    if !(200..300).contains(&status) {
        let diagnostic = read_upstream_diagnostic(upstream, &[&route.api_key, &route.client_token]);
        span.finish(Some(status), 0);
        respond_diagnostic(request, client_protocol, status, &diagnostic);
        return;
    }
    let mut diagnostic = response_diagnostic(
        &upstream,
        ProviderFailureKind::ResponseParse,
        "上游响应解析失败",
        &[&route.api_key, &route.client_token],
    );
    if requested_stream != upstream_stream {
        span.finish(Some(502), 0);
        diagnostic.kind = ProviderFailureKind::StreamParse;
        diagnostic.message = if requested_stream {
            "上游服务未按请求返回 SSE 流"
        } else {
            "上游服务返回了未请求的 SSE 流"
        }
        .to_string();
        respond_diagnostic(request, client_protocol, 502, &diagnostic);
        return;
    }
    if requested_stream {
        respond_sse(request, span, client_protocol, route, upstream, diagnostic);
        return;
    }
    respond_json(request, span, client_protocol, route, upstream, diagnostic);
}

fn respond_json(
    request: Request,
    span: RequestSpan,
    client_protocol: UpstreamProtocol,
    route: &ActiveRoute,
    mut upstream: UpstreamResponse,
    mut diagnostic: ProviderDiagnostic,
) {
    let status = upstream.status().as_u16();
    let headers = upstream.headers().clone();
    let encoded = match read_limited(&mut upstream, MAX_RESPONSE_BYTES) {
        Ok(body) => body,
        Err(ReadLimitError::TooLarge) => {
            span.finish(Some(502), 0);
            diagnostic.message = "上游响应超过本机协议网关限制".to_string();
            respond_diagnostic(request, client_protocol, 502, &diagnostic);
            return;
        }
        Err(ReadLimitError::Io) => {
            span.finish(Some(502), 0);
            diagnostic.kind = ProviderFailureKind::Network;
            diagnostic.message = "读取上游响应时连接中断".to_string();
            respond_diagnostic(request, client_protocol, 502, &diagnostic);
            return;
        }
    };
    let body = match content_encoding::decode_response_body(
        upstream.headers(),
        &encoded,
        MAX_RESPONSE_BYTES as usize,
    ) {
        Ok(body) => body,
        Err(error) => {
            span.finish(Some(502), 0);
            diagnostic.message = format!("无法解压上游响应：{error}");
            respond_diagnostic(request, client_protocol, 502, &diagnostic);
            return;
        }
    };
    let native = client_protocol == UpstreamProtocol::Responses
        && route.upstream_protocol == UpstreamProtocol::Responses;
    if !native {
        if let Some(error) =
            embedded_error(&body, &diagnostic, &[&route.api_key, &route.client_token])
        {
            span.finish(Some(502), 0);
            respond_diagnostic(request, client_protocol, 502, &error);
            return;
        }
    }
    let body = if native {
        match redact_native_body(&body, &[&route.api_key, &route.client_token]) {
            Ok(body) => body,
            Err(error) => {
                span.finish(Some(502), 0);
                diagnostic.message = error;
                respond_diagnostic(request, client_protocol, 502, &diagnostic);
                return;
            }
        }
    } else {
        body
    };
    let reasoning_transport = ReasoningTransport::from_continuation_key(route.continuation_key);
    let output = convert_response(
        route.upstream_protocol,
        client_protocol,
        &body,
        Some(&reasoning_transport),
    );
    match output {
        Ok(body) => {
            let client_status = native.then_some(status).unwrap_or(200);
            span.finish(Some(client_status), body.len() as u64);
            respond_bytes_with_headers(
                request,
                client_status,
                "application/json; charset=utf-8",
                body,
                native
                    .then(|| {
                        allowed_response_headers(&headers, &[&route.api_key, &route.client_token])
                    })
                    .unwrap_or_default(),
            )
        }
        Err(error) => {
            span.finish(Some(502), 0);
            diagnostic.message = crate::provider_diagnostics::redact_text(
                &format!("无法转换上游响应：{error}"),
                &[&route.api_key, &route.client_token],
            );
            respond_diagnostic(request, client_protocol, 502, &diagnostic);
        }
    }
}

fn respond_sse(
    request: Request,
    span: RequestSpan,
    client_protocol: UpstreamProtocol,
    route: &ActiveRoute,
    upstream: UpstreamResponse,
    mut diagnostic: ProviderDiagnostic,
) {
    diagnostic.kind = ProviderFailureKind::StreamParse;
    let reasoning_transport = ReasoningTransport::from_continuation_key(route.continuation_key);
    let headers = upstream.headers().clone();
    let native = client_protocol == UpstreamProtocol::Responses
        && route.upstream_protocol == UpstreamProtocol::Responses;
    let status = native.then_some(upstream.status().as_u16()).unwrap_or(200);
    let source = match content_encoding::decode_response_stream(
        &headers,
        upstream,
        MAX_RESPONSE_BYTES as usize,
    ) {
        Ok(source) => source,
        Err(error) => {
            span.finish(Some(502), 0);
            diagnostic.message = format!("无法解压上游 SSE：{error}");
            respond_diagnostic(request, client_protocol, 502, &diagnostic);
            return;
        }
    };
    match SseTranscoder::new(
        source,
        route.upstream_protocol,
        client_protocol,
        MAX_RESPONSE_BYTES,
        Some(&reasoning_transport),
    ) {
        Ok(stream) => respond_stream(
            request,
            span,
            stream.with_diagnostics(diagnostic, &[&route.api_key, &route.client_token]),
            status,
            // The stream remains under validation after the HTTP response
            // begins. Its later diagnostic cannot retract upstream headers,
            // so preserve no provider headers on this parsed path.
            native
                .then(|| allowed_response_headers(&headers, &[&route.api_key, &route.client_token]))
                .unwrap_or_default(),
        ),
        Err(error) => {
            span.finish(Some(502), 0);
            diagnostic.message = format!("无法转换上游 SSE：{error}");
            respond_diagnostic(request, client_protocol, 502, &diagnostic);
        }
    }
}

pub(crate) fn redact_native_body(body: &[u8], secrets: &[&str]) -> Result<Vec<u8>, String> {
    let text =
        std::str::from_utf8(body).map_err(|_| "原生 Responses 响应不是有效 UTF-8".to_string())?;
    Ok(crate::provider_diagnostics::redact_text(text, secrets).into_bytes())
}

fn respond_stream<R>(
    request: Request,
    span: RequestSpan,
    mut stream: SseTranscoder<R>,
    status: u16,
    mut headers: Vec<Header>,
) where
    R: Read,
{
    headers.push(content_type("text/event-stream; charset=utf-8"));
    headers.push(Header::from_bytes(b"Cache-Control", b"no-cache").expect("valid header"));
    let mut writer = match request.stream_response_with_headers(status, &headers) {
        Ok(writer) => writer,
        Err(_) => {
            span.finish(None, 0);
            return;
        }
    };
    let mut buffer = [0_u8; 8 * 1024];
    let mut written: u64 = 0;
    loop {
        let count = match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) => {
                stream.fail_io(&error);
                continue;
            }
        };
        if writer
            .write_all(&buffer[..count])
            .and_then(|_| writer.flush())
            .is_err()
        {
            span.finish(None, written);
            return;
        }
        written += count as u64;
    }
    let _ = writer.flush();
    span.finish(Some(if stream.failed() { 502 } else { status }), written);
}

pub(crate) enum ReadLimitError {
    TooLarge,
    Io,
}

/// Relays a typed Codex sibling operation which is already in the upstream
/// protocol. These operations have no Responses-to-other-protocol conversion
/// contract, so a successful upstream status and JSON body remain intact.
pub(crate) fn respond_passthrough(
    request: Request,
    span: RequestSpan,
    client_protocol: UpstreamProtocol,
    route: &ActiveRoute,
    mut upstream: UpstreamResponse,
) {
    let status = upstream.status().as_u16();
    let headers = upstream.headers().clone();
    if !(200..300).contains(&status) {
        let diagnostic = read_upstream_diagnostic(upstream, &[&route.api_key, &route.client_token]);
        span.finish(Some(status), 0);
        respond_diagnostic(request, client_protocol, status, &diagnostic);
        return;
    }
    let upstream_stream = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("text/event-stream"));
    if upstream_stream {
        let source = match content_encoding::decode_response_stream(
            &headers,
            upstream,
            MAX_RESPONSE_BYTES as usize,
        ) {
            Ok(source) => source,
            Err(error) => {
                span.finish(Some(502), 0);
                respond_error(
                    request,
                    Some(client_protocol),
                    502,
                    &format!("无法解压上游 SSE：{error}"),
                );
                return;
            }
        };
        respond_passthrough_sse(
            request,
            span,
            status,
            allowed_response_headers(&headers, &[&route.api_key, &route.client_token]),
            source,
        );
        return;
    }
    let encoded = match read_limited(&mut upstream, MAX_RESPONSE_BYTES) {
        Ok(body) => body,
        Err(ReadLimitError::TooLarge) => {
            span.finish(Some(502), 0);
            respond_error(
                request,
                Some(client_protocol),
                502,
                "上游响应超过本机协议网关限制",
            );
            return;
        }
        Err(ReadLimitError::Io) => {
            span.finish(Some(502), 0);
            respond_error(
                request,
                Some(client_protocol),
                502,
                "读取上游响应时连接中断",
            );
            return;
        }
    };
    let body = match content_encoding::decode_response_body(
        &headers,
        &encoded,
        MAX_RESPONSE_BYTES as usize,
    ) {
        Ok(body) => body,
        Err(error) => {
            span.finish(Some(502), 0);
            respond_error(
                request,
                Some(client_protocol),
                502,
                &format!("无法解压上游响应：{error}"),
            );
            return;
        }
    };
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/json; charset=utf-8");
    span.finish(Some(status), body.len() as u64);
    respond_bytes_with_headers(
        request,
        status,
        content_type,
        body,
        allowed_response_headers(&headers, &[&route.api_key, &route.client_token]),
    );
}

fn respond_passthrough_sse(
    request: Request,
    span: RequestSpan,
    status: u16,
    mut headers: Vec<Header>,
    mut source: Box<dyn Read>,
) {
    headers.push(content_type("text/event-stream; charset=utf-8"));
    headers.push(Header::from_bytes(b"Cache-Control", b"no-cache").expect("valid header"));
    let mut writer = match request.stream_response_with_headers(status, &headers) {
        Ok(writer) => writer,
        Err(_) => {
            span.finish(None, 0);
            return;
        }
    };
    let mut buffer = [0_u8; 8 * 1024];
    let mut written = 0_u64;
    loop {
        let count = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(_) => {
                span.finish(Some(502), written);
                return;
            }
        };
        written += count as u64;
        if written > MAX_RESPONSE_BYTES {
            span.finish(Some(502), written);
            return;
        }
        if writer
            .write_all(&buffer[..count])
            .and_then(|_| writer.flush())
            .is_err()
        {
            span.finish(None, written);
            return;
        }
    }
    let _ = writer.flush();
    span.finish(Some(status), written);
}

/// Reads and decodes an upstream failure before its body reaches provider
/// diagnostics. Every Codex transport uses this function so compressed error
/// envelopes cannot be mistaken for opaque upstream failures.
pub(crate) fn read_upstream_diagnostic(
    mut upstream: UpstreamResponse,
    secrets: &[&str],
) -> ProviderDiagnostic {
    let endpoint = upstream.url().to_string();
    let status = upstream.status().as_u16();
    let headers = upstream.headers().clone();
    let encoded = match read_limited(&mut upstream, MAX_DIAGNOSTIC_BODY_BYTES as u64) {
        Ok(body) => body,
        Err(ReadLimitError::TooLarge) => {
            return http_diagnostic(&endpoint, status, &headers, &[], true, secrets);
        }
        Err(ReadLimitError::Io) => {
            let mut diagnostic = http_diagnostic(&endpoint, status, &headers, &[], true, secrets);
            diagnostic.message.push_str("；读取上游错误响应时连接中断");
            return diagnostic;
        }
    };
    match content_encoding::decode_response_body(&headers, &encoded, MAX_DIAGNOSTIC_BODY_BYTES) {
        Ok(body) => http_diagnostic(&endpoint, status, &headers, &body, false, secrets),
        Err(error) => {
            let mut diagnostic = http_diagnostic(&endpoint, status, &headers, &[], true, secrets);
            diagnostic.message = format!("无法解压上游错误响应：{error}");
            diagnostic
        }
    }
}

pub(crate) fn read_limited(reader: &mut dyn Read, limit: u64) -> Result<Vec<u8>, ReadLimitError> {
    let mut output = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut output)
        .map_err(|_| ReadLimitError::Io)?;
    if output.len() as u64 > limit {
        return Err(ReadLimitError::TooLarge);
    }
    Ok(output)
}

pub(crate) fn respond_error(
    request: Request,
    protocol: Option<UpstreamProtocol>,
    status: u16,
    message: &str,
) {
    let body = protocol
        .map(|protocol| convert_error(protocol, status, message))
        .unwrap_or_else(|| convert_error(UpstreamProtocol::ChatCompletions, status, message));
    let response = Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(content_type("application/json; charset=utf-8"));
    let _ = request.respond(response);
}

pub(super) fn respond_bytes(
    request: Request,
    status: u16,
    content_type_value: &str,
    body: Vec<u8>,
) {
    respond_bytes_with_headers(request, status, content_type_value, body, Vec::new());
}

fn respond_bytes_with_headers(
    request: Request,
    status: u16,
    content_type_value: &str,
    body: Vec<u8>,
    mut headers: Vec<Header>,
) {
    headers.push(content_type(content_type_value));
    let response = Response::new(StatusCode(status), headers, Cursor::new(body), None, None);
    let _ = request.respond(response);
}

fn allowed_response_headers(headers: &HeaderMap, secrets: &[&str]) -> Vec<Header> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            matches!(
                name.as_str(),
                "x-request-id"
                    | "retry-after"
                    | "openai-processing-ms"
                    | "x-ratelimit-limit-requests"
                    | "x-ratelimit-remaining-requests"
                    | "x-ratelimit-reset-requests"
                    | "x-ratelimit-limit-tokens"
                    | "x-ratelimit-remaining-tokens"
                    | "x-ratelimit-reset-tokens"
            )
            .then(|| {
                let value = value.to_str().ok()?;
                let value = crate::provider_diagnostics::redact_text(value, secrets);
                Header::from_bytes(name.as_str(), value.as_bytes()).ok()
            })
            .flatten()
        })
        .collect()
}

pub(crate) fn content_type(value: &str) -> Header {
    Header::from_bytes(&b"Content-Type"[..], value.as_bytes()).expect("valid content type")
}
