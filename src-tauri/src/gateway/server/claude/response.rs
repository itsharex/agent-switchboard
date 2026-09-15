//! Validate the first Claude response before committing the client HTTP status.

use super::*;
use crate::gateway::transform::SseTranscoder;
use std::io::Read;

pub(super) enum Prepared {
    Json(UpstreamResponse),
    Stream(SseTranscoder<Box<dyn Read>>),
}

impl Prepared {
    pub(super) fn new(
        upstream: UpstreamResponse,
        route: &ActiveRoute,
        stream: bool,
    ) -> Result<Self, ProviderDiagnostic> {
        if !stream {
            return prepare_non_stream_response(upstream, route).map(Self::Json);
        }
        let mut diagnostic = super::super::diagnostics::response_diagnostic(
            &upstream,
            ProviderFailureKind::StreamParse,
            "Claude 上游未返回有效的 SSE 首帧",
            &[&route.api_key, &route.client_token],
        );
        let content_type = upstream
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        if !upstream.initial_body_received()
            || !content_type
                .to_ascii_lowercase()
                .starts_with("text/event-stream")
        {
            return Err(diagnostic);
        }
        let headers = upstream.headers().clone();
        let source = content_encoding::decode_response_stream(
            &headers,
            upstream,
            MAX_RESPONSE_BYTES as usize,
        )
        .map_err(|error| {
            diagnostic.message = format!("无法解压 Claude SSE：{error}");
            diagnostic.clone()
        })?;
        let reasoning = ReasoningTransport::from_continuation_key(route.continuation_key);
        let mut stream = SseTranscoder::new(
            source,
            route.upstream_protocol,
            UpstreamProtocol::AnthropicMessages,
            MAX_RESPONSE_BYTES,
            Some(&reasoning),
        )
        .map_err(|error| {
            diagnostic.message = error.to_string();
            diagnostic.clone()
        })?
        .with_diagnostics(diagnostic, &[&route.api_key, &route.client_token]);
        stream.prime()?;
        Ok(Self::Stream(stream))
    }
    pub(super) fn respond(self, request: Request, span: RequestSpan, route: &ActiveRoute) {
        match self {
            Self::Json(upstream) => respond_upstream(
                request,
                span,
                UpstreamProtocol::AnthropicMessages,
                route,
                false,
                upstream,
                None,
            ),
            Self::Stream(stream) => {
                super::super::respond::respond_stream(request, span, stream, 200, Vec::new(), None)
            }
        }
    }
}
