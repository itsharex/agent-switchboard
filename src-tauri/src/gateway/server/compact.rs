use super::*;
use std::io::Write;
use tiny_http::Header;

pub(super) fn respond(
    request: Request,
    mut span: RequestSpan,
    route: &ActiveRoute,
    inner: &GatewayInner,
    client: &UpstreamClient,
    body: &[u8],
    legacy: bool,
) {
    let stream = match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(value) => value.get("stream").and_then(serde_json::Value::as_bool),
        Err(_) => None,
    };
    // The V2 contract is an SSE response on the ordinary /responses route.
    // Keep the legacy /responses/compact JSON contract unchanged, including
    // its historical tolerance for a `stream` field.
    if !legacy && stream != Some(true) {
        let diagnostic = ProviderDiagnostic::new(
            ProviderFailureKind::RequestParameters,
            &inner.configured_base_url(),
            "Responses compaction_trigger 请求必须启用 stream=true",
        );
        span.finish(Some(422), 0);
        respond_diagnostic(request, UpstreamProtocol::Responses, 422, &diagnostic);
        return;
    }
    let settings = match crate::gateway::codex::policy::load(&inner.state_root) {
        Ok((policy, _)) => policy.traffic,
        Err(error) => {
            span.finish(Some(503), 0);
            respond_error(request, Some(UpstreamProtocol::Responses), 503, &error);
            return;
        }
    };
    let result = match super::super::compaction::execute(
        client,
        route,
        &inner.configured_base_url(),
        request.url(),
        body,
        Some(request.headers()),
        settings.timeouts(false),
        Some(&|| request.is_cancelled()),
    ) {
        Ok(result) => result,
        Err(diagnostic) => {
            let status = diagnostic_status(&diagnostic);
            span.note_codex_attempt(route, Some(status), false);
            span.finish(Some(status), 0);
            respond_diagnostic(request, UpstreamProtocol::Responses, status, &diagnostic);
            return;
        }
    };
    span.note_codex_attempt(route, Some(200), false);
    span.note_mapped_model(result.upstream_model.clone());
    span.note_response_value(UpstreamProtocol::Responses, &result.response);
    let (content, mime) = render_result(result, legacy, stream);
    let size = content.len() as u64;
    let headers = [Header::from_bytes("Content-Type", mime).expect("static header")];
    let mut writer = match request.stream_response_with_headers(200, &headers) {
        Ok(writer) => writer,
        Err(_) => {
            span.finish(None, 0);
            return;
        }
    };
    let delivered = writer
        .write_all(content.as_bytes())
        .and_then(|_| writer.flush())
        .is_ok();
    // Keep the body open until durable accounting finishes, so a sequential
    // caller cannot outrun the in-flight slots with completed compact responses.
    span.finish(delivered.then_some(200), size);
}

fn render_result(
    result: crate::gateway::compaction::CompactionResult,
    legacy: bool,
    stream: Option<bool>,
) -> (String, &'static str) {
    if !legacy && stream == Some(true) {
        let events = result
            .events()
            .into_iter()
            .map(|event| {
                format!(
                    "event: {}\ndata: {event}\n\n",
                    event["type"].as_str().unwrap_or("message")
                )
            })
            .collect::<String>();
        (events, "text/event-stream")
    } else {
        (
            if legacy {
                result.legacy()
            } else {
                result.response
            }
            .to_string(),
            "application/json",
        )
    }
}

fn diagnostic_status(diagnostic: &ProviderDiagnostic) -> u16 {
    if let Some(status) = diagnostic.status {
        if status == 401 {
            return 502;
        }
        if (400..=599).contains(&status) {
            return status;
        }
    }
    match diagnostic.kind {
        ProviderFailureKind::RequestParameters => 422,
        ProviderFailureKind::RateLimit => 429,
        _ => 502,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(kind: ProviderFailureKind, status: Option<u16>) -> ProviderDiagnostic {
        let mut diagnostic = ProviderDiagnostic::new(kind, "http://127.0.0.1/upstream", "error");
        diagnostic.status = status;
        diagnostic
    }

    #[test]
    fn compact_errors_keep_upstream_request_and_rate_limit_statuses() {
        assert_eq!(
            diagnostic_status(&diagnostic(ProviderFailureKind::Upstream, Some(422))),
            422
        );
        assert_eq!(
            diagnostic_status(&diagnostic(ProviderFailureKind::RateLimit, Some(429))),
            429
        );
    }

    #[test]
    fn compact_oauth_status_isolated_to_gateway_failure() {
        assert_eq!(
            diagnostic_status(&diagnostic(ProviderFailureKind::Authentication, Some(401))),
            502
        );
    }

    #[test]
    fn local_compaction_parameter_failures_use_unprocessable_entity() {
        assert_eq!(
            diagnostic_status(&diagnostic(ProviderFailureKind::RequestParameters, None)),
            422
        );
    }
}
