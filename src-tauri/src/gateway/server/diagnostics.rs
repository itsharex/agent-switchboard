use super::respond::content_type;
use super::UpstreamResponse;
use crate::gateway::http::Request;
use crate::provider_diagnostics::{http_diagnostic, ProviderDiagnostic, ProviderFailureKind};
use asb_core::contracts::UpstreamProtocol;
use serde_json::{json, Value};
use tiny_http::{Header, Response, StatusCode};

pub(super) fn embedded_error(
    body: &[u8],
    context: &ProviderDiagnostic,
    secrets: &[&str],
) -> Option<ProviderDiagnostic> {
    let value: Value = serde_json::from_slice(body).ok()?;
    if !value.get("error").is_some_and(|error| !error.is_null()) {
        return None;
    }
    let mut diagnostic = http_diagnostic(
        &context.endpoint,
        context.status.unwrap_or(200),
        &reqwest::header::HeaderMap::new(),
        body,
        false,
        secrets,
    );
    diagnostic.request_id = context.request_id.clone();
    Some(diagnostic)
}

pub(super) fn response_diagnostic(
    upstream: &UpstreamResponse,
    kind: ProviderFailureKind,
    message: &str,
    secrets: &[&str],
) -> ProviderDiagnostic {
    let mut diagnostic = http_diagnostic(
        upstream.url().as_str(),
        upstream.status().as_u16(),
        upstream.headers(),
        &[],
        false,
        secrets,
    );
    diagnostic.kind = kind;
    diagnostic.message = message.to_string();
    diagnostic
}

pub(super) fn respond_diagnostic(
    request: Request,
    protocol: UpstreamProtocol,
    status: u16,
    diagnostic: &ProviderDiagnostic,
) {
    let mut error = diagnostic.error_value();
    let status = if diagnostic.status == Some(401) && protocol == UpstreamProtocol::Responses {
        error["code"] = json!("upstream_authentication_failed");
        error["upstreamStatus"] = json!(401);
        error["clientStatus"] = json!(502);
        502
    } else {
        status
    };
    let envelope = match protocol {
        UpstreamProtocol::AnthropicMessages => {
            error["type"] = json!("api_error");
            json!({"type":"error", "error":error})
        }
        UpstreamProtocol::Responses => json!({"object":"error", "status":status, "error":error}),
        UpstreamProtocol::ChatCompletions => json!({"error":error}),
    };
    let mut response = Response::from_data(envelope.to_string().into_bytes())
        .with_status_code(StatusCode(status))
        .with_header(content_type("application/json; charset=utf-8"));
    if let Some(id) = &diagnostic.request_id {
        if let Ok(header) = Header::from_bytes(b"x-request-id", id.as_bytes()) {
            response.add_header(header);
        }
    }
    if let Some(retry_after) = &diagnostic.retry_after {
        if let Ok(header) = Header::from_bytes(b"retry-after", retry_after.as_bytes()) {
            response.add_header(header);
        }
    }
    let _ = request.respond(response);
}
