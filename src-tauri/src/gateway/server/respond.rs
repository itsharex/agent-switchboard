//! Upstream response handling: converted JSON bodies, chunked SSE relaying,
//! error rendering, and the shared size-limited reader.

use super::super::metrics::RequestSpan;
use super::super::transform::{convert_error, convert_response, ReasoningTransport, SseTranscoder};
use super::super::ActiveRoute;
use super::diagnostics::{embedded_error, respond_diagnostic, response_diagnostic};
use super::MAX_RESPONSE_BYTES;
use crate::gateway::http::Request;
use crate::provider_diagnostics::{read_http_diagnostic, ProviderDiagnostic, ProviderFailureKind};
use asb_core::contracts::UpstreamProtocol;
use reqwest::blocking::Response as UpstreamResponse;
use reqwest::header::CONTENT_TYPE;
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
        let diagnostic = read_http_diagnostic(upstream, &[&route.api_key, &route.client_token]);
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
    let body = match read_limited(&mut upstream, MAX_RESPONSE_BYTES) {
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
    if let Some(error) = embedded_error(&body, &diagnostic, &[&route.api_key, &route.client_token])
    {
        span.finish(Some(502), 0);
        respond_diagnostic(request, client_protocol, 502, &error);
        return;
    }
    let reasoning_transport = ReasoningTransport::from_continuation_key(route.continuation_key);
    let output = convert_response(
        route.upstream_protocol,
        client_protocol,
        &body,
        Some(&reasoning_transport),
    );
    match output {
        Ok(body) => {
            span.finish(Some(200), body.len() as u64);
            respond_bytes(request, 200, "application/json; charset=utf-8", body)
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
    match SseTranscoder::new(
        upstream,
        route.upstream_protocol,
        client_protocol,
        MAX_RESPONSE_BYTES,
        Some(&reasoning_transport),
    ) {
        Ok(stream) => respond_stream(
            request,
            span,
            stream.with_diagnostics(diagnostic, &[&route.api_key, &route.client_token]),
        ),
        Err(error) => {
            span.finish(Some(502), 0);
            diagnostic.message = format!("无法转换上游 SSE：{error}");
            respond_diagnostic(request, client_protocol, 502, &diagnostic);
        }
    }
}

fn respond_stream<R>(request: Request, span: RequestSpan, mut stream: SseTranscoder<R>)
where
    R: Read,
{
    let mut writer = match request.stream_response(
        200,
        &[
            ("Content-Type", "text/event-stream; charset=utf-8"),
            ("Cache-Control", "no-cache"),
        ],
    ) {
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
    span.finish(Some(if stream.failed() { 502 } else { 200 }), written);
}

pub(crate) enum ReadLimitError {
    TooLarge,
    Io,
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

fn respond_bytes(request: Request, status: u16, content_type_value: &str, body: Vec<u8>) {
    let response = Response::new(
        StatusCode(status),
        vec![content_type(content_type_value)],
        Cursor::new(body),
        None,
        None,
    );
    let _ = request.respond(response);
}

pub(crate) fn content_type(value: &str) -> Header {
    Header::from_bytes(&b"Content-Type"[..], value.as_bytes()).expect("valid content type")
}
