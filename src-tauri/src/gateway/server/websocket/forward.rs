//! WebSocket delivery around the same Codex attempts and policy as HTTP.
use super::*;
use super::super::codex_forward::{self, attempt};
use crate::gateway::codex::policy::CodexGatewayPolicy;

pub(super) fn execute<S: Read + Write>(
    socket: &mut WebSocket<S>,
    client: &UpstreamClient,
    inner: &GatewayInner,
    primary: &ActiveRoute,
    candidates: Vec<ActiveRoute>,
    context: &mut ConversationContext,
    request: PendingRequest,
    request_url: &str,
    headers: &[Header],
    span: &mut RequestSpan,
) -> ExchangeOutcome {
    let policy = match codex_forward::admit(inner, primary, &request.body) {
        Ok(policy) => policy,
        Err((_, message)) => {
            let diagnostic = request_diagnostic(inner, primary, request_url, &message);
            return reject_diagnostic(socket, &diagnostic);
        }
    };
    let limit = codex_forward::attempt_limit(&policy, primary);
    Forward {
        socket,
        client,
        inner,
        context,
        request,
        request_url,
        headers,
        span,
        policy,
        limit,
        media_retried: false,
    }
    .run(candidates)
}

struct Forward<'a, S: Read + Write> {
    socket: &'a mut WebSocket<S>,
    client: &'a UpstreamClient,
    inner: &'a GatewayInner,
    context: &'a mut ConversationContext,
    request: PendingRequest,
    request_url: &'a str,
    headers: &'a [Header],
    span: &'a mut RequestSpan,
    policy: CodexGatewayPolicy,
    limit: usize,
    media_retried: bool,
}

enum Reply {
    Upstream(UpstreamResponse, PendingRequest),
    Compact(crate::gateway::compaction::CompactionResult, PendingRequest),
}

impl<S: Read + Write> Forward<'_, S> {
    fn run(mut self, candidates: Vec<ActiveRoute>) -> ExchangeOutcome {
        let mut last_failure = None;
        let mut attempted = 0;
        for candidate in candidates {
            if attempted >= self.limit || self.inner.stopping.load(Ordering::Acquire) {
                break;
            }
            let route = match attempt::resolve_route(&candidate, self.inner, self.headers) {
                Ok(route) => route,
                Err(failure) => return reject_diagnostic(self.socket, &failure.diagnostic),
            };
            let health = self.inner.codex_health
                .health_for(&route, self.policy.traffic.health_config());
            let Some(permit) = health.try_acquire() else {
                continue;
            };
            attempted += 1;
            match self.send_with_media_retry(&route) {
                Ok(reply) => {
                    let outcome = self.deliver(&route, reply);
                    match outcome {
                        ExchangeOutcome::Served => {
                            permit.record_success();
                            self.inner.record_codex_endpoint_success(&route);
                        }
                        ExchangeOutcome::Rejected => permit.record_failure(),
                        _ => drop(permit),
                    }
                    return outcome;
                }
                Err(failure) => {
                    if failure.penalize {
                        permit.record_failure();
                    } else {
                        drop(permit);
                    }
                    let retry = failure.retryable || (failure.status == 422 && !failure.penalize);
                    last_failure = Some(failure);
                    if !retry {
                        break;
                    }
                }
            }
        }
        match last_failure {
            Some(failure) => reject_diagnostic(self.socket, &failure.diagnostic),
            None => {
                if send_failed(self.socket, "gateway_unavailable", "Codex 供应商均处于熔断冷却状态或没有可用候选") {
                    ExchangeOutcome::Rejected
                } else {
                    ExchangeOutcome::Disconnect
                }
            }
        }
    }

    fn send_with_media_retry(&mut self, route: &ActiveRoute) -> Result<Reply, attempt::Failure> {
        let failure = match self.send(route) {
            Ok(reply) => return Ok(reply),
            Err(failure) => failure,
        };
        let Some(body) = attempt::media_fallback_body(
            &self.request.body,
            &failure,
            self.policy.media_fallback,
            &mut self.media_retried,
        ) else {
            return Err(failure);
        };
        let original = std::mem::replace(&mut self.request.body, body);
        let result = self.send(route);
        self.request.body = original;
        result
    }

    fn send(&mut self, route: &ActiveRoute) -> Result<Reply, attempt::Failure> {
        self.span.bind_route(&route.profile_id, &route.fingerprint, route.upstream_protocol);
        self.span.protect_secrets(&[&route.api_key, &route.client_token]);
        self.span.admit_codex_route(route, true)
            .map_err(|(status, message)| attempt::Failure {
                diagnostic: ProviderDiagnostic::new(
                    ProviderFailureKind::RateLimit, &route.upstream_base_url, &message,
                ),
                status,
                retryable: true,
                penalize: false,
            })?;
        let result = self.send_attempt(route);
        match &result {
            Ok(_) => self.span.note_codex_attempt(route, Some(200), false),
            Err(failure) => self.span.note_codex_attempt(route, Some(failure.status), failure.retryable),
        }
        result
    }

    fn send_attempt(&mut self, route: &ActiveRoute) -> Result<Reply, attempt::Failure> {
        let mut request = self.request
            .for_upstream(route.upstream_protocol == UpstreamProtocol::Responses);
        let base = self.inner.configured_base_url();
        let inner = self.inner;
        let cancelled = || inner.stopping.load(Ordering::Acquire);
        if route.upstream_protocol != UpstreamProtocol::Responses
            && crate::gateway::compaction::is_v2(&request.body).unwrap_or(false)
        {
            request.body = super::super::codex::resolve_model_and_validate(
                route, CodexOperation::Responses, request.body,
            ).map_err(|message| attempt::Failure {
                diagnostic: request_diagnostic(inner, route, self.request_url, &message),
                status: 422, retryable: false, penalize: false,
            })?;
            return crate::gateway::compaction::execute(
                self.client, route, &base, self.request_url, &request.body, Some(self.headers),
                self.policy.traffic.timeouts(false), Some(&cancelled),
            ).map(|result| Reply::Compact(result, request)).map_err(attempt::classify);
        }
        let prepared = attempt::prepare(
            route, &base, self.request_url, &request.body, self.headers, &self.inner.codex_history,
        )?;
        self.span.note_mapped_model(prepared.model.clone());
        attempt::send(self.client, route, self.headers, &cancelled, prepared, &self.policy.traffic)
            .map(|(upstream, _)| Reply::Upstream(upstream, request))
    }

    fn deliver(&mut self, route: &ActiveRoute, reply: Reply) -> ExchangeOutcome {
        match reply {
            Reply::Upstream(upstream, request) if request.stream => {
                exchange::relay_stream(self.socket, upstream, route, self.context, &request, self.span)
            }
            Reply::Upstream(upstream, request) => {
                exchange::relay_response(self.socket, upstream, route, self.context, &request, self.span)
            }
            Reply::Compact(result, request) => self.deliver_compact(result, &request),
        }
    }

    fn deliver_compact(
        &mut self,
        result: crate::gateway::compaction::CompactionResult,
        request: &PendingRequest,
    ) -> ExchangeOutcome {
        self.span.note_mapped_model(result.upstream_model.clone());
        self.span.note_response_value(UpstreamProtocol::Responses, &result.response);
        if let Err(error) = self.context.record_completed(request, &result.response) {
            return if send_failed(self.socket, error.code, &error.message) {
                ExchangeOutcome::Rejected
            } else {
                ExchangeOutcome::Disconnect
            };
        }
        for event in result.events() {
            if !send_value(self.socket, &event) {
                return ExchangeOutcome::Disconnect;
            }
        }
        ExchangeOutcome::Served
    }
}
