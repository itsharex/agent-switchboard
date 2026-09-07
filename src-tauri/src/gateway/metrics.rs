//! In-memory request telemetry for the loopback gateway. Samples exist only
//! for the status page: nothing about gateway traffic is persisted, and a
//! sample never carries credentials — only the profile id that owned the
//! route.

use asb_core::contracts::{AppKind, UpstreamProtocol};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Bounded recent-request buffer. Older samples are dropped once full; the
/// cumulative counters remain exact so totals never silently lag the chart.
const SAMPLE_CAPACITY: usize = 512;

/// One completed gateway request, recorded exactly once at the boundary that
/// answered the client. `status` is the HTTP status for direct HTTP traffic
/// and `Some(200)` / `None` for a served / failed WebSocket message exchange.
/// `profile_id` and `upstream_protocol` are set only when the capability
/// token matched a route; rejected or unmatched traffic stays unattributed
/// instead of wearing placeholder values.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GatewaySample {
    pub(crate) at_ms: u64,
    pub(crate) app: AppKind,
    pub(crate) profile_id: Option<String>,
    pub(crate) client_protocol: UpstreamProtocol,
    pub(crate) upstream_protocol: Option<UpstreamProtocol>,
    pub(crate) status: Option<u16>,
    pub(crate) duration_ms: u64,
    pub(crate) request_bytes: u64,
    pub(crate) response_bytes: u64,
}

impl GatewaySample {
    fn failed(&self) -> bool {
        self.status.is_none_or(|status| status >= 400)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GatewayMetricsSnapshot {
    pub(crate) started_at_ms: u64,
    pub(crate) total_requests: u64,
    pub(crate) failed_requests: u64,
    pub(crate) samples: Vec<GatewaySample>,
}

pub(crate) struct GatewayMetrics {
    started_at_ms: u64,
    total_requests: AtomicU64,
    failed_requests: AtomicU64,
    samples: Mutex<VecDeque<GatewaySample>>,
}

impl GatewayMetrics {
    pub(crate) fn new() -> Self {
        Self {
            started_at_ms: unix_now_ms(),
            total_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            samples: Mutex::new(VecDeque::new()),
        }
    }

    pub(crate) fn record(&self, sample: GatewaySample) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        if sample.failed() {
            self.failed_requests.fetch_add(1, Ordering::Relaxed);
        }
        // A poisoned lock means a panic elsewhere already failed the process;
        // the deque append itself keeps no invariant that poisoning breaks,
        // so recovery never drops telemetry.
        let mut samples = self
            .samples
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if samples.len() >= SAMPLE_CAPACITY {
            samples.pop_front();
        }
        samples.push_back(sample);
    }

    pub(crate) fn snapshot(&self) -> GatewayMetricsSnapshot {
        let samples = self
            .samples
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .cloned()
            .collect();
        GatewayMetricsSnapshot {
            started_at_ms: self.started_at_ms,
            total_requests: self.total_requests.load(Ordering::Relaxed),
            failed_requests: self.failed_requests.load(Ordering::Relaxed),
            samples,
        }
    }
}

/// Collects the facts of one in-flight request so its outcome is recorded
/// exactly once, at the boundary that answers the client. The route is bound
/// only after the capability token matches, so rejected traffic is counted
/// without attributing it to any profile.
pub(crate) struct RequestSpan {
    metrics: Arc<GatewayMetrics>,
    app: AppKind,
    client_protocol: UpstreamProtocol,
    profile_id: Option<String>,
    upstream_protocol: Option<UpstreamProtocol>,
    request_bytes: u64,
    started: Instant,
}

impl RequestSpan {
    pub(crate) fn start(
        metrics: Arc<GatewayMetrics>,
        app: AppKind,
        client_protocol: UpstreamProtocol,
    ) -> Self {
        Self {
            metrics,
            app,
            client_protocol,
            profile_id: None,
            upstream_protocol: None,
            request_bytes: 0,
            started: Instant::now(),
        }
    }

    pub(crate) fn bind_route(&mut self, profile_id: &str, upstream_protocol: UpstreamProtocol) {
        self.profile_id = Some(profile_id.to_string());
        self.upstream_protocol = Some(upstream_protocol);
    }

    pub(crate) fn note_request_bytes(&mut self, bytes: u64) {
        self.request_bytes = bytes;
    }

    /// `status` is `None` only when no HTTP status applies to the failure, as
    /// with a WebSocket exchange that died before an answer was delivered.
    pub(crate) fn finish(self, status: Option<u16>, response_bytes: u64) {
        self.metrics.record(GatewaySample {
            at_ms: unix_now_ms(),
            app: self.app,
            profile_id: self.profile_id,
            client_protocol: self.client_protocol,
            upstream_protocol: self.upstream_protocol,
            status,
            duration_ms: self.started.elapsed().as_millis() as u64,
            request_bytes: self.request_bytes,
            response_bytes,
        });
    }
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        rejected.bind_route("p1", UpstreamProtocol::ChatCompletions);
        rejected.finish(Some(401), 0);
        span(&metrics).finish(None, 0);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_requests, 3);
        assert_eq!(snapshot.failed_requests, 2);
        assert_eq!(snapshot.samples.len(), 3);
        let unbound = &snapshot.samples[2];
        assert_eq!(unbound.profile_id, None);
        assert_eq!(unbound.upstream_protocol, None);
        let sample = &snapshot.samples[1];
        assert_eq!(sample.profile_id.as_deref(), Some("p1"));
        assert_eq!(
            sample.upstream_protocol,
            Some(UpstreamProtocol::ChatCompletions)
        );
        assert_eq!(sample.status, Some(401));
        assert!(sample.failed());
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
}
