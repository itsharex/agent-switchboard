//! Forward converted Responses events without discarding a terminal failure.

use super::*;

pub(super) fn send_stream<S: Read + Write, R: Read>(
    socket: &mut WebSocket<S>,
    mut stream: SseTranscoder<R>,
    route: &ActiveRoute,
    context: &mut ConversationContext,
    request: &PendingRequest,
    mut diagnostic: ProviderDiagnostic,
    span: &mut RequestSpan,
) -> ExchangeOutcome {
    let mut decoder = ResponseEventDecoder::default();
    let mut completed = false;
    let mut failed = false;
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) => {
                stream.fail_io(&error);
                continue;
            }
        };
        span.note_first_byte();
        if stream.has_token() {
            span.note_first_token();
        }
        span.note_response_model(stream.model());
        span.note_usage(stream.usage());
        let events = match decoder.push(&buffer[..count]) {
            Ok(events) => events,
            Err(()) => {
                diagnostic.message = "无法解析转换后的 SSE 流".to_string();
                return reject_diagnostic(socket, &diagnostic);
            }
        };
        for event in events {
            let kind = event
                .value
                .get("type")
                .and_then(Value::as_str)
                .filter(|kind| event.name.as_deref().is_none_or(|name| name == *kind));
            let Some(kind) = kind.filter(|kind| {
                route.upstream_protocol == UpstreamProtocol::Responses
                    || known_responses_event(kind)
            }) else {
                diagnostic.message = "转换后的 SSE 事件无效或不受支持".to_string();
                return reject_diagnostic(socket, &diagnostic);
            };
            if kind == "response.completed" {
                if let Err(message) = record_completion(route, context, request, &event.value) {
                    diagnostic.message = message;
                    return reject_diagnostic(socket, &diagnostic);
                }
                completed = true;
            } else if matches!(kind, "response.failed" | "response.incomplete") {
                failed = true;
            }
            if !send_value(socket, &event.value) {
                return ExchangeOutcome::Disconnect;
            }
        }
    }
    if decoder.finish().is_err() || (!completed && !failed) {
        diagnostic.message = "上游 SSE 流未完整结束或没有终止事件".to_string();
        return reject_diagnostic(socket, &diagnostic);
    }
    if failed {
        ExchangeOutcome::Rejected
    } else {
        ExchangeOutcome::Served
    }
}

fn record_completion(
    _route: &ActiveRoute,
    context: &mut ConversationContext,
    request: &PendingRequest,
    event: &Value,
) -> Result<(), String> {
    let response = event
        .get("response")
        .ok_or("response.completed 缺少响应对象")?;
    context
        .record_completed(request, response)
        .map_err(|error| error.message)?;
    Ok(())
}
