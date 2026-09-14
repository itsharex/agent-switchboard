use super::*;
use crate::gateway::request_ledger::ClaudeFailoverOutcome;

fn span(metrics: &Arc<GatewayMetrics>) -> RequestSpan {
    RequestSpan::start(
        Arc::clone(metrics),
        AppKind::Codex,
        UpstreamProtocol::Responses,
    )
}

#[test]
fn counts_totals_and_failures_while_keeping_recent_samples_only() {
    let metrics = Arc::new(GatewayMetrics::new());
    span(&metrics).finish(Some(200), 10);
    let mut rejected = span(&metrics);
    rejected.bind_route("p1", "revision-p1", UpstreamProtocol::ChatCompletions);
    rejected.finish(Some(401), 0);
    span(&metrics).finish(None, 0);

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.total_requests, 3);
    assert_eq!(snapshot.failed_requests, 2);
    assert_eq!(snapshot.samples.len(), 3);
    let unbound = &snapshot.samples[2];
    assert_eq!(unbound.profile_id, None);
    assert_eq!(unbound.route_revision, None);
    assert_eq!(unbound.upstream_protocol, None);
    let sample = &snapshot.samples[1];
    assert_eq!(sample.profile_id.as_deref(), Some("p1"));
    assert_eq!(sample.route_revision.as_deref(), Some("revision-p1"));
    assert_eq!(
        sample.upstream_protocol,
        Some(UpstreamProtocol::ChatCompletions)
    );
    assert_eq!(sample.status, Some(401));
    assert!(sample.failed());

    let rendered = serde_json::to_value(sample).expect("serialize metric sample");
    assert_eq!(rendered["routeRevision"], "revision-p1");
}

#[test]
fn snapshot_evicts_oldest_samples_but_keeps_cumulative_counts() {
    let metrics = Arc::new(GatewayMetrics::new());
    for _ in 0..(SAMPLE_CAPACITY + 10) {
        span(&metrics).finish(Some(200), 0);
    }
    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.samples.len(), SAMPLE_CAPACITY);
    assert_eq!(snapshot.total_requests, (SAMPLE_CAPACITY + 10) as u64);
    assert_eq!(snapshot.failed_requests, 0);
}

#[test]
fn keeps_each_accepted_requests_bound_route_revision() {
    let metrics = Arc::new(GatewayMetrics::new());
    let mut before_switch = span(&metrics);
    before_switch.bind_route("p1", "revision-a", UpstreamProtocol::Responses);
    before_switch.finish(Some(200), 0);

    let mut after_switch = span(&metrics);
    after_switch.bind_route("p1", "revision-b", UpstreamProtocol::Responses);
    after_switch.finish(Some(200), 0);

    let snapshot = metrics.snapshot();
    let revisions: Vec<_> = snapshot
        .samples
        .iter()
        .map(|sample| sample.route_revision.as_deref())
        .collect();
    assert_eq!(revisions, [Some("revision-a"), Some("revision-b")]);
}

#[test]
fn claude_span_persists_metadata_and_failover_attempts() {
    let directory = tempfile::tempdir().unwrap();
    let ledger = Arc::new(ClaudeRequestLedger::new(
        directory.path().join("state/claude-request-ledger.json"),
    ));
    let metrics = Arc::new(GatewayMetrics::new());
    let mut span = RequestSpan::start_with_ledger(
        metrics,
        Arc::clone(&ledger),
        AppKind::Claude,
        UpstreamProtocol::AnthropicMessages,
    );
    span.bind_route("profile-1", "revision-1", UpstreamProtocol::ChatCompletions);
    span.note_request_model(Some("claude-sonnet".to_string()));
    span.note_mapped_model(Some("vendor-sonnet".to_string()));
    span.note_usage(TokenUsage {
        input_tokens: Some(10),
        output_tokens: Some(4),
        cache_read_tokens: Some(2),
        cache_creation_tokens: Some(1),
        reasoning_tokens: Some(3),
    });
    span.note_first_byte();
    span.note_failover_attempt(ClaudeFailoverAttempt {
        profile_id: "profile-1".to_string(),
        route_revision: "revision-1".to_string(),
        upstream_protocol: UpstreamProtocol::ChatCompletions,
        status: Some(503),
        outcome: ClaudeFailoverOutcome::RetryableFailure,
    });
    span.finish(Some(200), 64);

    let page = ledger.page(0, 1, None).unwrap();
    let entry = &page.entries[0];
    assert_eq!(entry.request_model.as_deref(), Some("claude-sonnet"));
    assert_eq!(entry.input_tokens, Some(10));
    assert_eq!(entry.cache_read_tokens, Some(2));
    assert_eq!(entry.failover_attempts.len(), 1);
    assert_eq!(entry.status, Some(200));
}
