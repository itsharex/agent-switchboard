//! Upstream response handling: converted JSON bodies, chunked SSE relaying,
//! error rendering, and the shared size-limited reader.

use super::super::metrics::RequestSpan;
use super::super::transform::{convert_error, convert_response, ReasoningTransport, SseTranscoder};
use super::super::ActiveRoute;
use super::MAX_RESPONSE_BYTES;
use asb_core::contracts::UpstreamProtocol;
use reqwest::blocking::Response as UpstreamResponse;
use reqwest::header::CONTENT_TYPE;
use std::io::{Cursor, Read, Write};
use tiny_http::{Header, Request, Response, StatusCode};

pub(crate) fn respond_upstream(
    request: Request,
    span: RequestSpan,
    client_protocol: UpstreamProtocol,
    route: &ActiveRoute,
    upstream_protocol: UpstreamProtocol,
    requested_stream: bool,
    mut upstream: UpstreamResponse,
) {
    let status = upstream.status().as_u16();
    let upstream_stream = upstream
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("text/event-stream"));
    if !(200..300).contains(&status) {
        span.finish(Some(status), 0);
        respond_request_error(
            request,
            client_protocol,
            status,
            &format!("上游服务返回 HTTP {status}"),
        );
        return;
    }
    if requested_stream != upstream_stream {
        span.finish(Some(502), 0);
        respond_request_error(
            request,
            client_protocol,
            502,
            if requested_stream {
                "上游服务未按请求返回 SSE 流"
            } else {
                "上游服务返回了未请求的 SSE 流"
            },
        );
        return;
    }
    if requested_stream {
        let reasoning_transport = ReasoningTransport::from_client_token(&route.client_token);
        let stream = match SseTranscoder::new(
            upstream,
            upstream_protocol,
            client_protocol,
            MAX_RESPONSE_BYTES,
            Some(&reasoning_transport),
        ) {
            Ok(stream) => stream,
            Err(error) => {
                span.finish(Some(502), 0);
                respond_request_error(
                    request,
                    client_protocol,
                    502,
                    &format!("无法转换上游 SSE：{error}"),
                );
                return;
            }
        };
        respond_stream(request, span, stream);
        return;
    }
    let body = match read_limited(&mut upstream, MAX_RESPONSE_BYTES) {
        Ok(body) => body,
        Err(ReadLimitError::TooLarge) => {
            span.finish(Some(502), 0);
            respond_request_error(
                request,
                client_protocol,
                502,
                "上游响应超过本机协议网关限制",
            );
            return;
        }
        Err(ReadLimitError::Io) => {
            span.finish(Some(502), 0);
            respond_request_error(request, client_protocol, 502, "无法读取上游响应");
            return;
        }
    };
    let reasoning_transport = ReasoningTransport::from_client_token(&route.client_token);
    let output = convert_response(
        upstream_protocol,
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
            respond_request_error(
                request,
                client_protocol,
                502,
                &format!("无法转换上游响应：{error}"),
            )
        }
    }
}

fn respond_stream<R>(request: Request, span: RequestSpan, mut stream: SseTranscoder<R>)
where
    R: Read,
{
    // tiny_http's `Response` reader is flushed only after the whole body has
    // been copied. SSE must flush every converted frame, so this narrow raw
    // response writer is the HTTP boundary for streamed routes only.
    let mut writer = request.into_writer();
    if writer
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n",
        )
        .and_then(|_| writer.flush())
        .is_err()
    {
        span.finish(None, 0);
        return;
    }
    let mut buffer = [0_u8; 8 * 1024];
    let mut written: u64 = 0;
    loop {
        let count = match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(_) => {
                span.finish(None, written);
                return;
            }
        };
        if write!(writer, "{count:x}\r\n")
            .and_then(|_| writer.write_all(&buffer[..count]))
            .and_then(|_| writer.write_all(b"\r\n"))
            .and_then(|_| writer.flush())
            .is_err()
        {
            span.finish(None, written);
            return;
        }
        written += count as u64;
    }
    let _ = writer.write_all(b"0\r\n\r\n");
    let _ = writer.flush();
    span.finish(Some(200), written);
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

fn respond_request_error(request: Request, protocol: UpstreamProtocol, status: u16, message: &str) {
    respond_bytes(
        request,
        status,
        "application/json; charset=utf-8",
        convert_error(protocol, status, message),
    );
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
