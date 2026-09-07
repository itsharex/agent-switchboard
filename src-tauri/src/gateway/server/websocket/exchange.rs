//! Upstream exchange execution: request relay and streamed/non-streamed answers.

use super::*;

pub(super) fn execute_request<S>(
    socket: &mut WebSocket<S>,
    client: &Client,
    gateway_base: &str,
    route: &ActiveRoute,
    context: &mut ConversationContext,
    request: PendingRequest,
) -> bool
where
    S: Read + Write,
{
    if request.body.len() as u64 > MAX_REQUEST_BYTES {
        return send_failed(socket, "request_too_large", "请求体超过本机协议网关限制");
    }
    let reasoning_transport = ReasoningTransport::from_client_token(&route.client_token);
    let converted = match convert_request(
        UpstreamProtocol::Responses,
        route.upstream_protocol,
        &request.body,
        route.max_output_tokens,
        Some(&reasoning_transport),
    ) {
        Ok(converted) => converted,
        Err(_) => {
            return send_failed(socket, "invalid_request", "无法转换 Codex WebSocket 请求");
        }
    };
    if converted.body.len() as u64 > MAX_REQUEST_BYTES {
        return send_failed(
            socket,
            "request_too_large",
            "转换后的请求体超过本机协议网关限制",
        );
    }
    if converted.stream != request.stream {
        return send_failed(
            socket,
            "gateway_error",
            "转换后的请求流状态与 Codex WebSocket 请求不一致",
        );
    }
    let url = match upstream_url(route, gateway_base) {
        Ok(url) => url,
        Err(_) => return send_failed(socket, "gateway_error", "无法形成上游请求地址"),
    };
    let upstream = match client
        .post(url)
        .headers(upstream_headers(route, None, None))
        .body(converted.body)
        .send()
    {
        Ok(response) => response,
        Err(_) => return send_failed(socket, "connection_error", "无法连接上游服务"),
    };
    let status = upstream.status().as_u16();
    if !(200..300).contains(&status) {
        return send_failed(socket, "upstream_error", "上游服务拒绝了请求");
    }
    let upstream_stream = upstream
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("text/event-stream"));
    if request.stream != upstream_stream {
        return send_failed(
            socket,
            "gateway_error",
            if request.stream {
                "上游服务未按请求返回 SSE 流"
            } else {
                "上游服务返回了未请求的 SSE 流"
            },
        );
    }
    if request.stream {
        relay_stream(socket, upstream, route, context, &request)
    } else {
        relay_response(socket, upstream, route, context, &request)
    }
}

pub(super) fn relay_response<S>(
    socket: &mut WebSocket<S>,
    mut upstream: UpstreamResponse,
    route: &ActiveRoute,
    context: &mut ConversationContext,
    request: &PendingRequest,
) -> bool
where
    S: Read + Write,
{
    let body = match read_limited(&mut upstream, MAX_RESPONSE_BYTES) {
        Ok(body) => body,
        Err(_) => return send_failed(socket, "gateway_error", "无法读取上游响应"),
    };
    let reasoning_transport = ReasoningTransport::from_client_token(&route.client_token);
    let body = match convert_response(
        route.upstream_protocol,
        UpstreamProtocol::Responses,
        &body,
        Some(&reasoning_transport),
    ) {
        Ok(body) => body,
        Err(_) => return send_failed(socket, "gateway_error", "无法转换上游响应"),
    };
    let response: Value = match serde_json::from_slice(&body) {
        Ok(response) => response,
        Err(_) => return send_failed(socket, "gateway_error", "转换后的响应不是有效 JSON"),
    };
    if context.record_completed(request, &response).is_err() {
        return send_failed(socket, "gateway_error", "无法安全保存上游响应上下文");
    }
    send_created_and_completed(socket, &response)
}

pub(super) fn relay_stream<S>(
    socket: &mut WebSocket<S>,
    upstream: UpstreamResponse,
    route: &ActiveRoute,
    context: &mut ConversationContext,
    request: &PendingRequest,
) -> bool
where
    S: Read + Write,
{
    let reasoning_transport = ReasoningTransport::from_client_token(&route.client_token);
    let mut stream = match SseTranscoder::new(
        upstream,
        route.upstream_protocol,
        UpstreamProtocol::Responses,
        MAX_RESPONSE_BYTES,
        Some(&reasoning_transport),
    ) {
        Ok(stream) => stream,
        Err(_) => return send_failed(socket, "gateway_error", "无法转换上游 SSE 流"),
    };
    let mut decoder = ResponseEventDecoder::default();
    let mut completed = false;
    let mut failed = false;
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(_) => return send_failed(socket, "gateway_error", "无法读取上游 SSE 流"),
        };
        let events = match decoder.push(&buffer[..count]) {
            Ok(events) => events,
            Err(_) => return send_failed(socket, "gateway_error", "无法解析转换后的 SSE 流"),
        };
        for event in events {
            let kind = event
                .value
                .get("type")
                .and_then(Value::as_str)
                .filter(|kind| event.name.as_deref().is_none_or(|name| name == *kind));
            let Some(kind) = kind else {
                return send_failed(socket, "gateway_error", "转换后的 SSE 事件无效");
            };
            if !known_responses_event(kind) {
                return send_failed(socket, "gateway_error", "转换后的 SSE 事件不受支持");
            }
            match kind {
                "response.completed" => {
                    let Some(response) = event.value.get("response") else {
                        return send_failed(
                            socket,
                            "gateway_error",
                            "response.completed 缺少响应对象",
                        );
                    };
                    // The stream transformer created this event from the active
                    // request, so cache the context only after its final strict
                    // representation is available.
                    if context.record_completed(request, response).is_err() {
                        return send_failed(socket, "gateway_error", "无法安全保存上游响应上下文");
                    }
                    if !send_value(socket, &event.value) {
                        return false;
                    }
                    completed = true;
                    let _ = response;
                }
                "response.failed" => {
                    if !send_value(socket, &event.value) {
                        return false;
                    }
                    failed = true;
                }
                _ => {
                    if !send_value(socket, &event.value) {
                        return false;
                    }
                }
            }
        }
    }
    if decoder.finish().is_err() {
        return send_failed(socket, "gateway_error", "转换后的 SSE 流未完整结束");
    }
    if completed || failed {
        return true;
    }
    send_failed(socket, "gateway_error", "上游 SSE 流未给出终止事件")
}
