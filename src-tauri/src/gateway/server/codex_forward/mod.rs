//! Codex Responses failover. Every candidate re-renders the original request
//! with its own immutable credentials/model snapshot; delivered streams never retry.
pub(super) mod attempt;
mod operations;
use super::*;
use crate::gateway::codex::policy::{self, CodexGatewayPolicy};
use crate::gateway::provider_health::RequestPermit;
pub(super) use operations::forward as forward_operation;
use std::sync::atomic::Ordering;

pub(super) fn respond(
    request: Request,
    mut span: RequestSpan,
    primary: ActiveRoute,
    candidates: Vec<ActiveRoute>,
    inner: Arc<GatewayInner>,
    client: Arc<UpstreamClient>,
    body: Vec<u8>,
) {
    let requested_model = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|value| {
            value
                .get("model")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });
    // A subagent wire model swaps the whole candidate list for its referenced
    // profile and strips the wire prefix, so admission and per-attempt
    // rendering validate against the target's own catalog and credentials.
    let (primary, candidates, body) =
        match resolve_subagent_routing(&inner, requested_model.as_deref(), body) {
            Ok(SubagentOutcome::NotRouted(body)) => (primary, candidates, body),
            Ok(SubagentOutcome::Routed {
                route,
                variants,
                body,
            }) => (route, variants, body),
            Err((status, message)) => {
                span.finish(Some(status), 0);
                respond_error(request, Some(UpstreamProtocol::Responses), status, &message);
                return;
            }
        };
    let policy = match admit(&inner, &primary, &body) {
        Ok(policy) => policy,
        Err((status, message)) => {
            span.finish(Some(status), 0);
            respond_error(request, Some(UpstreamProtocol::Responses), status, &message);
            return;
        }
    };
    if let Some(model) = requested_model {
        span.note_request_model(Some(model));
    }
    let limit = attempt_limit(&policy, &primary);
    Forward {
        request,
        span,
        inner,
        client,
        body,
        policy,
        limit,
        last_failure: None,
        media_retried: false,
    }
    .run(candidates);
}

pub(super) enum SubagentOutcome {
    NotRouted(Vec<u8>),
    Routed {
        route: ActiveRoute,
        variants: Vec<ActiveRoute>,
        body: Vec<u8>,
    },
}

/// Parses the subagent wire spelling. Any model carrying the `asb:` prefix
/// but failing to parse fails closed here — it can never resolve as an
/// ordinary catalog id either.
pub(super) fn resolve_subagent_routing(
    inner: &GatewayInner,
    requested_model: Option<&str>,
    body: Vec<u8>,
) -> Result<SubagentOutcome, (u16, String)> {
    use asb_core::contracts::CodexSubagentRoute;
    let Some(model) = requested_model else {
        return Ok(SubagentOutcome::NotRouted(body));
    };
    let parse_failure = |model: &str| {
        (
            422u16,
            format!("请求模型 {model} 使用了 asb: 前缀但不是有效的子代理路由引用"),
        )
    };
    let Some(wire) = CodexSubagentRoute::parse_wire_id(model) else {
        return if model.starts_with(CodexSubagentRoute::WIRE_PREFIX) {
            Err(parse_failure(model))
        } else {
            Ok(SubagentOutcome::NotRouted(body))
        };
    };
    if !wire.has_canonical_profile_id() {
        return Err(parse_failure(model));
    }
    let (route, variants) = inner
        .subagent_route_candidates(&wire)
        .map_err(|message| (422u16, message))?;
    let mut value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| (422u16, "Codex 请求体不是有效 JSON".to_string()))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| (422u16, "Codex 请求体必须是 JSON 对象".to_string()))?;
    object.insert("model".to_string(), serde_json::Value::String(wire.model));
    let body = serde_json::to_vec(&value)
        .map_err(|_| (422u16, "Codex 请求序列化失败".to_string()))?;
    Ok(SubagentOutcome::Routed {
        route,
        variants,
        body,
    })
}

pub(super) fn attempt_limit(policy: &CodexGatewayPolicy, primary: &ActiveRoute) -> usize {
    if policy.enabled {
        policy.max_retries as usize + 1
    } else {
        primary
            .connection
            .endpoint_candidates(&primary.upstream_base_url)
            .len()
            .max(1)
    }
}

pub(super) fn admit(
    inner: &GatewayInner,
    route: &ActiveRoute,
    body: &[u8],
) -> Result<CodexGatewayPolicy, (u16, String)> {
    if policy::pending_path(&inner.state_root).exists() {
        return Err((503, "Codex 网关策略事务尚未完成，请先恢复".into()));
    }
    let (policy, _) = policy::load(&inner.state_root).map_err(|error| (503, error))?;
    inner
        .codex_health
        .ensure_ready()
        .map_err(|error| (503, error))?;
    super::codex::resolve_model_and_validate(route, CodexOperation::Responses, body.to_vec())
        .map_err(|error| (422, error))?;
    Ok(policy)
}

struct Forward {
    request: Request,
    span: RequestSpan,
    inner: Arc<GatewayInner>,
    client: Arc<UpstreamClient>,
    body: Vec<u8>,
    policy: CodexGatewayPolicy,
    limit: usize,
    last_failure: Option<attempt::Failure>,
    media_retried: bool,
}
impl Forward {
    fn run(mut self, candidates: Vec<ActiveRoute>) {
        let mut attempted = 0;
        for candidate in candidates {
            if attempted >= self.limit || self.request.is_cancelled() {
                break;
            }
            let candidate = match attempt::resolve_route(&candidate, &self.inner, self.request.headers()) {
                Ok(route) => route,
                Err(failure) => {
                    self.last_failure = Some(failure);
                    break;
                }
            };
            let health = self
                .inner
                .codex_health
                .health_for(&candidate, self.policy.traffic.health_config());
            let Some(permit) = health.try_acquire() else {
                continue;
            };
            let Some(prepared) = self.prepare(&candidate) else {
                drop(permit);
                continue;
            };
            attempted += 1;
            match self.send(&candidate, prepared) {
                Ok((upstream, stream)) => {
                    self.deliver(&candidate, permit, upstream, stream);
                    return;
                }
                Err(failure) => {
                    if self.request.is_cancelled() {
                        drop(permit);
                        self.span.finish(None, 0);
                        return;
                    }
                    let failure = match self.media_downgrade(&candidate, failure) {
                        Ok(failure) => failure,
                        Err((upstream, stream)) => {
                            self.deliver(&candidate, permit, upstream, stream);
                            return;
                        }
                    };
                    if failure.penalize {
                        permit.record_failure();
                    } else {
                        drop(permit);
                    }
                    let retry = failure.retryable;
                    self.last_failure = Some(failure);
                    if !retry {
                        break;
                    }
                }
            }
        }
        self.finish();
    }

    fn prepare(&mut self, route: &ActiveRoute) -> Option<attempt::Prepared> {
        match self.prepare_body(route, &self.body) {
            Ok(prepared) => Some(prepared),
            Err(failure) => {
                if self.last_failure.is_none() {
                    self.last_failure = Some(failure);
                }
                None
            }
        }
    }
    fn prepare_body(
        &self,
        route: &ActiveRoute,
        body: &[u8],
    ) -> Result<attempt::Prepared, attempt::Failure> {
        attempt::prepare(
            route,
            &self.inner.configured_base_url(),
            self.request.url(),
            body,
            self.request.headers(),
            &self.inner.codex_history,
        )
    }
    /// One retry per request with image blocks replaced by a text marker
    /// after an upstream modality rejection. Success delivers through the
    /// same permit; a failed retry becomes the recorded failure and the
    /// original body resumes for any remaining candidate.
    fn media_downgrade(
        &mut self,
        route: &ActiveRoute,
        failure: attempt::Failure,
    ) -> Result<attempt::Failure, (UpstreamResponse, bool)> {
        let Some(media_body) = attempt::media_fallback_body(
            &self.body, &failure, self.policy.media_fallback, &mut self.media_retried,
        ) else { return Ok(failure) };
        let original = std::mem::replace(&mut self.body, media_body);
        let outcome = match self.prepare_body(route, &self.body) {
            Ok(prepared) => self.send(route, prepared),
            Err(retry_failure) => Err(retry_failure),
        };
        self.body = original;
        match outcome {
            Ok((upstream, stream)) => Err((upstream, stream)),
            Err(retry_failure) => Ok(retry_failure),
        }
    }
    fn send(
        &mut self,
        route: &ActiveRoute,
        prepared: attempt::Prepared,
    ) -> Result<(UpstreamResponse, bool), attempt::Failure> {
        self.span.bind_route(
            &route.profile_id,
            &route.fingerprint,
            route.upstream_protocol,
        );
        self.span.note_mapped_model(prepared.model.clone());
        self.span
            .admit_codex_route(route, true)
            .map_err(|(status, message)| attempt::Failure {
                diagnostic: ProviderDiagnostic::new(
                    ProviderFailureKind::RateLimit,
                    &route.upstream_base_url,
                    &message,
                ),
                status,
                retryable: true,
                penalize: false,
            })?;
        let outcome = attempt::send(
            &self.client,
            route,
            self.request.headers(),
            &|| self.request.is_cancelled(),
            prepared,
            &self.policy.traffic,
        );
        match &outcome {
            Ok((upstream, _)) => {
                self.span
                    .note_codex_attempt(route, Some(upstream.status().as_u16()), false)
            }
            Err(failure) => {
                self.span
                    .note_codex_attempt(route, Some(failure.status), failure.retryable)
            }
        }
        outcome
    }
    fn deliver(
        mut self,
        route: &ActiveRoute,
        permit: RequestPermit<'_>,
        upstream: UpstreamResponse,
        stream: bool,
    ) {
        let completed = self.span.observe_completion();
        let binding = crate::gateway::codex::history::HistoryBinding::for_request(
            &self.body,
            Some(self.request.headers()),
        );
        let recorder = crate::gateway::codex::history::StreamRecorder::new(
            self.inner.codex_history.clone(),
            binding,
        );
        respond_upstream(
            self.request,
            self.span,
            UpstreamProtocol::Responses,
            route,
            stream,
            upstream,
            Some(recorder),
        );
        match completed.load(Ordering::Acquire) {
            200..=299 => {
                permit.record_success();
                self.inner.record_codex_endpoint_success(route);
            }
            0 => drop(permit),
            status if crate::gateway::is_retryable_http_status(status) => permit.record_failure(),
            _ => drop(permit),
        }
    }
    fn finish(self) {
        if self.request.is_cancelled() {
            self.span.finish(None, 0);
            return;
        }
        if let Some(failure) = self.last_failure {
            self.span.finish(Some(failure.status), 0);
            respond_diagnostic(
                self.request,
                UpstreamProtocol::Responses,
                failure.status,
                &failure.diagnostic,
            );
        } else {
            self.span.finish(Some(503), 0);
            respond_error(
                self.request,
                Some(UpstreamProtocol::Responses),
                503,
                "Codex 供应商均处于熔断冷却状态或没有可用候选",
            );
        }
    }
}
