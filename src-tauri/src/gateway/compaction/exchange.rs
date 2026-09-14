use super::*;
use crate::gateway::{server, transform, ActiveRoute};
use crate::provider_diagnostics::{ProviderDiagnostic, ProviderFailureKind};
use asb_core::contracts::UpstreamProtocol;
use std::io::Read;
use tiny_http::Header;

pub(crate) fn execute(
    client: &server::UpstreamClient,
    route: &ActiveRoute,
    gateway_base: &str,
    request_url: &str,
    body: &[u8],
    incoming: Option<&[Header]>,
) -> Result<CompactionResult, ProviderDiagnostic> {
    let url = server::upstream_url(route, gateway_base, request_url).map_err(|message| {
        ProviderDiagnostic::new(
            ProviderFailureKind::Endpoint,
            &route.upstream_base_url,
            &message,
        )
    })?;
    let invalid = |error: TransformError| {
        ProviderDiagnostic::new(ProviderFailureKind::RequestParameters, &url, &error.0)
    };
    let request = request::prepare(
        body,
        &route.continuation_key,
        route.max_output_tokens,
        route.upstream_protocol,
    )
    .map_err(invalid)?;
    let transport = transform::ReasoningTransport::from_continuation_key(route.continuation_key);
    let mut request = transform::convert_request(
        UpstreamProtocol::Responses,
        route.upstream_protocol,
        &request,
        route.max_output_tokens,
        Some(&transport),
        route
            .codex
            .as_ref()
            .map(|snapshot| &snapshot.capabilities.chat_reasoning),
    )
    .and_then(|request| transform::minimal::apply(request, route.responses_options))
    .map_err(invalid)?;
    let beta = crate::gateway::codex::request::prepare(route, body, &mut request.body, incoming)
        .map_err(|error| invalid(TransformError(error)))?;
    let upstream_model =
        crate::gateway::usage_metadata::model_from_bytes(route.upstream_protocol, &request.body);
    if request.body.len() as u64 > server::MAX_REQUEST_BYTES {
        return Err(invalid(TransformError("压缩历史超过网关请求预算".into())));
    }
    let upstream = server::send_upstream_request(
        client,
        route,
        &url,
        reqwest::Method::POST,
        request.body,
        incoming,
        None,
        beta.as_deref(),
    )?;
    if !upstream.status().is_success() {
        return Err(server::read_upstream_diagnostic(
            upstream,
            &[&route.api_key, &route.client_token],
        ));
    }
    let mut result = complete_upstream(upstream, route, &url, &transport)?;
    result.upstream_model = upstream_model;
    Ok(result)
}

fn complete_upstream(
    upstream: server::UpstreamResponse,
    route: &ActiveRoute,
    url: &str,
    transport: &transform::ReasoningTransport,
) -> Result<CompactionResult, ProviderDiagnostic> {
    let mut diagnostic =
        ProviderDiagnostic::new(ProviderFailureKind::ResponseParse, &url, "压缩摘要失败");
    diagnostic.status = Some(upstream.status().as_u16());
    diagnostic.request_id = crate::provider_diagnostics::request_id(upstream.headers()).map(|id| {
        crate::provider_diagnostics::redact_text(&id, &[&route.api_key, &route.client_token])
    });
    let headers = upstream.headers().clone();
    let mut body = Vec::new();
    if upstream
        .take(server::MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut body)
        .is_err()
    {
        diagnostic.kind = ProviderFailureKind::Network;
        diagnostic.message = "读取压缩摘要时连接中断".into();
        return Err(diagnostic);
    }
    if body.len() as u64 > server::MAX_RESPONSE_BYTES {
        diagnostic.message = "压缩响应超过网关限制".into();
        return Err(diagnostic);
    }
    let body = match crate::gateway::content_encoding::decode_response_body(
        &headers,
        &body,
        server::MAX_RESPONSE_BYTES as usize,
    ) {
        Ok(body) => body,
        Err(error) => {
            diagnostic.message = format!("无法解压上游压缩响应：{error}");
            return Err(diagnostic);
        }
    };
    let result = validate_completion(&body, route.upstream_protocol)
        .and_then(|_| {
            transform::convert_response(
                route.upstream_protocol,
                UpstreamProtocol::Responses,
                &body,
                Some(transport),
            )
        })
        .and_then(|body| finish(&body, &route.continuation_key));
    result.map_err(|error| {
        diagnostic.message = error.0;
        diagnostic
    })
}

fn validate_completion(body: &[u8], protocol: UpstreamProtocol) -> Result<(), TransformError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| TransformError("压缩模型没有返回 JSON 摘要".into()))?;
    let complete = match protocol {
        UpstreamProtocol::Responses => value["status"] == "completed",
        UpstreamProtocol::ChatCompletions => value["choices"]
            .as_array()
            .is_some_and(|choices| choices.len() == 1 && choices[0]["finish_reason"] == "stop"),
        UpstreamProtocol::AnthropicMessages => value["stop_reason"] == "end_turn",
        UpstreamProtocol::GeminiGenerateContent => return Err(TransformError("Codex 压缩不能使用 Claude 专用 Gemini 上游".into())),
    };
    if !complete || value.get("error").is_some_and(|v| !v.is_null()) {
        return Err(TransformError(
            "压缩摘要被截断或未正常结束；原历史保持不变".into(),
        ));
    }
    Ok(())
}
