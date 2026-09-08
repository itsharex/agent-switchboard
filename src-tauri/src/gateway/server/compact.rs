use super::*;
use tiny_http::{Header, Response, StatusCode};

pub(super) fn respond(
    request: Request,
    span: RequestSpan,
    route: &ActiveRoute,
    inner: &GatewayInner,
    client: &Client,
    body: &[u8],
    legacy: bool,
) {
    let result = match super::super::compaction::execute(
        client,
        route,
        &inner.configured_base_url(),
        body,
    ) {
        Ok(result) => result,
        Err(diagnostic) => {
            span.finish(Some(502), 0);
            respond_diagnostic(request, UpstreamProtocol::Responses, 502, &diagnostic);
            return;
        }
    };
    let stream = !legacy
        && serde_json::from_slice::<serde_json::Value>(body)
            .ok()
            .is_some_and(|value| value["stream"] == true);
    let (content, mime) = if stream {
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
    };
    let size = content.len() as u64;
    let response = Response::from_string(content)
        .with_status_code(StatusCode(200))
        .with_header(Header::from_bytes("Content-Type", mime).expect("static header"));
    let delivered = request.respond(response).is_ok();
    span.finish(delivered.then_some(200), size);
}
