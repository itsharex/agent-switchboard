use super::*;
use crate::gateway::codex::policy::CodexTrafficSettings;
use std::time::Duration;
use tiny_http::Header;

pub(super) struct Prepared {
    url: String,
    converted: ConvertedRequest,
    pub model: Option<String>,
    anthropic_beta: Option<String>,
}
pub(super) struct Failure {
    pub diagnostic: ProviderDiagnostic,
    pub status: u16,
    pub retryable: bool,
    pub penalize: bool,
}

pub(super) fn prepare(
    route: &ActiveRoute,
    gateway_base: &str,
    request_url: &str,
    body: &[u8],
    incoming: &[Header],
) -> Result<Prepared, Failure> {
    let invalid = |message: String| Failure {
        status: 422,
        retryable: false,
        penalize: false,
        diagnostic: ProviderDiagnostic::new(
            ProviderFailureKind::RequestParameters,
            &route.upstream_base_url,
            &message,
        ),
    };
    let body = super::super::codex::resolve_model_and_validate(
        route,
        CodexOperation::Responses,
        body.to_vec(),
    )
    .map_err(invalid)?;
    let key = ReasoningTransport::from_continuation_key(route.continuation_key);
    let mut converted = convert_request(
        UpstreamProtocol::Responses,
        route.upstream_protocol,
        &body,
        route.max_output_tokens,
        Some(&key),
        route.codex.as_ref().map(|s| &s.capabilities.chat_reasoning),
    )
    .and_then(|request| crate::gateway::transform::minimal::apply(request, route.responses_options))
    .map_err(|error| invalid(error.to_string()))?;
    let anthropic_beta = crate::gateway::codex::request::prepare(route, &body, &mut converted.body, Some(incoming)).map_err(invalid)?;
    let model =
        crate::gateway::usage_metadata::model_from_bytes(route.upstream_protocol, &converted.body);
    let url = upstream_url(route, gateway_base, request_url).map_err(invalid)?;
    Ok(Prepared {
        url,
        converted,
        model,
        anthropic_beta,
    })
}

pub(super) fn send(
    client: &UpstreamClient,
    route: &ActiveRoute,
    request: &Request,
    prepared: Prepared,
    settings: &CodexTrafficSettings,
) -> Result<(UpstreamResponse, bool), Failure> {
    let stream = prepared.converted.stream;
    let timeouts = super::super::transport::Timeouts {
        headers: Duration::from_secs(settings.headers_timeout_seconds.into()),
        first_byte: Duration::from_secs(settings.first_byte_timeout_seconds.into()),
        idle: Duration::from_secs(settings.idle_timeout_seconds.into()),
        total: Duration::from_secs(settings.total_timeout_seconds.into()),
    };
    let response = super::super::transport::send_with_timeouts(
        client,
        route,
        &prepared.url,
        reqwest::Method::POST,
        prepared.converted.body,
        Some(request.headers()),
        None,
        prepared.anthropic_beta.as_deref(),
        timeouts,
        Some(&|| request.is_cancelled()),
    )
    .map_err(classify)?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let diagnostic = read_upstream_diagnostic(response, &[&route.api_key, &route.client_token]);
        return Err(Failure {
            diagnostic,
            status,
            retryable: crate::gateway::is_retryable_http_status(status),
            penalize: crate::gateway::is_retryable_http_status(status),
        });
    }
    let upstream_stream = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("text/event-stream"));
    if stream != upstream_stream {
        return Err(classify(ProviderDiagnostic::new(
            ProviderFailureKind::StreamParse,
            &prepared.url,
            "Codex 上游没有按请求返回对应的 JSON/SSE 格式",
        )));
    }
    if stream && !response.initial_body_received() {
        return Err(classify(ProviderDiagnostic::new(
            ProviderFailureKind::Network,
            &prepared.url,
            "Codex 上游流在首包前结束",
        )));
    }
    let response = if stream {
        response
    } else {
        prepare_non_stream_response(response, route).map_err(classify)?
    };
    Ok((response, stream))
}

fn classify(diagnostic: ProviderDiagnostic) -> Failure {
    let retryable = match diagnostic.kind {
        ProviderFailureKind::Dns
        | ProviderFailureKind::Tls
        | ProviderFailureKind::Network
        | ProviderFailureKind::Timeout
        | ProviderFailureKind::RateLimit
        | ProviderFailureKind::ResponseParse
        | ProviderFailureKind::StreamParse => true,
        ProviderFailureKind::Upstream => diagnostic
            .status
            .is_some_and(crate::gateway::is_retryable_http_status),
        _ => false,
    };
    let status = diagnostic.status.filter(|status| *status >= 400).unwrap_or(
        if diagnostic.kind == ProviderFailureKind::Timeout {
            504
        } else {
            502
        },
    );
    Failure {
        diagnostic,
        status,
        retryable,
        penalize: retryable,
    }
}
