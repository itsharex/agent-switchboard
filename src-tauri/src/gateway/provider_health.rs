//! Thread-safe, in-memory health state for one Claude upstream provider.
//!
//! This module intentionally has no gateway or persistence dependencies. A
//! caller acquires a permit before sending a request, then records success or
//! failure on that permit. The permit's generation prevents an older request
//! from changing a newer circuit state after a concurrent failure opens it.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The externally visible circuit state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderHealthState {
    Closed,
    Open,
    HalfOpen,
}

/// Credential-free state persisted for one stable Claude route key.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProviderHealthSnapshot {
    pub(crate) state: ProviderHealthState,
    pub(crate) consecutive_failures: u32,
    pub(crate) open_until_ms: Option<u64>,
    #[serde(default)]
    pub(crate) consecutive_successes: u32,
    #[serde(default)]
    pub(crate) total_requests: u32,
    #[serde(default)]
    pub(crate) failed_requests: u32,
}

impl Default for ProviderHealthSnapshot {
    fn default() -> Self {
        Self {
            state: ProviderHealthState::Closed,
            consecutive_failures: 0,
            open_until_ms: None,
            consecutive_successes: 0,
            total_requests: 0,
            failed_requests: 0,
        }
    }
}

/// Configuration for one provider health tracker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderHealthConfig {
    pub failure_threshold: u32,
    pub open_cooldown: Duration,
    pub success_threshold: u32,
    pub error_rate_percent: u32,
    pub min_requests: u32,
}

impl ProviderHealthConfig {
    /// Creates a config. A zero threshold is normalized to one failure.
    pub fn new(failure_threshold: u32, open_cooldown: Duration) -> Self {
        Self {
            failure_threshold: failure_threshold.max(1),
            open_cooldown,
            success_threshold: 1,
            error_rate_percent: 60,
            min_requests: 10,
        }
    }
}

impl Default for ProviderHealthConfig {
    fn default() -> Self {
        Self::new(3, Duration::from_secs(30))
    }
}

/// Returns true for upstream HTTP failures that can be retried.
pub fn is_retryable_http_status(status: u16) -> bool {
    (400..=599).contains(&status)
        && !matches!(status, 400 | 405 | 406 | 413 | 414 | 415 | 422 | 499 | 501)
}

/// The result of trying to reserve one upstream request.
#[must_use]
pub struct RequestPermit<'a> {
    health: &'a ProviderHealth,
    generation: u64,
    probe: bool,
    completed: bool,
}

impl RequestPermit<'_> {
    /// Records a successful request using the current monotonic clock.
    pub fn record_success(self) {
        self.record_success_at(Instant::now());
    }

    /// Records a failed request using the current monotonic clock.
    pub fn record_failure(self) {
        self.record_failure_at(Instant::now());
    }

    /// Deterministic-clock variant, useful for tests and clock-aware callers.
    pub fn record_success_at(mut self, now: Instant) {
        self.complete(Outcome::Success, now);
    }

    /// Deterministic-clock variant, useful for tests and clock-aware callers.
    pub fn record_failure_at(mut self, now: Instant) {
        self.complete(Outcome::Failure, now);
    }

    fn complete(&mut self, outcome: Outcome, now: Instant) {
        if self.completed {
            return;
        }
        self.completed = true;
        self.health
            .finish(self.generation, self.probe, outcome, now);
    }
}

impl Drop for RequestPermit<'_> {
    fn drop(&mut self) {
        if !self.completed {
            self.completed = true;
            self.health
                .release(self.generation, self.probe, Instant::now());
        }
    }
}

/// A synchronous and thread-safe health tracker for one upstream provider.
pub struct ProviderHealth {
    config: ProviderHealthConfig,
    inner: Mutex<Inner>,
    persistence: Option<Arc<dyn Fn(ProviderHealthSnapshot) + Send + Sync>>,
}

impl ProviderHealth {
    pub fn new(config: ProviderHealthConfig) -> Self {
        Self::with_snapshot_and_persistence(config, None, None)
    }

    pub(crate) fn with_snapshot_and_persistence(
        config: ProviderHealthConfig,
        snapshot: Option<ProviderHealthSnapshot>,
        persistence: Option<Arc<dyn Fn(ProviderHealthSnapshot) + Send + Sync>>,
    ) -> Self {
        Self::with_snapshot_at(config, snapshot, persistence, Instant::now(), unix_now_ms())
    }

    pub(crate) fn with_snapshot_at(
        config: ProviderHealthConfig,
        snapshot: Option<ProviderHealthSnapshot>,
        persistence: Option<Arc<dyn Fn(ProviderHealthSnapshot) + Send + Sync>>,
        now: Instant,
        now_ms: u64,
    ) -> Self {
        Self {
            config,
            inner: Mutex::new(Inner::from_snapshot(
                snapshot,
                config.open_cooldown,
                now,
                now_ms,
            )),
            persistence,
        }
    }

    pub(crate) fn reset(&self) {
        let mut inner = self.lock();
        let generation = inner.generation.wrapping_add(1);
        *inner = Inner::closed();
        inner.generation = generation;
    }

    pub fn config(&self) -> ProviderHealthConfig {
        self.config
    }

    /// Returns the current state. An expired open circuit is reported as half-open
    /// even before the first caller claims its probe permit.
    pub fn state(&self) -> ProviderHealthState {
        self.state_at(Instant::now())
    }

    pub fn state_at(&self, now: Instant) -> ProviderHealthState {
        let inner = self.lock();
        visible_state(&inner, self.config.open_cooldown, now)
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.lock().consecutive_failures
    }

    /// Returns the serializable state without exposing a monotonic timestamp.
    pub(crate) fn snapshot(&self) -> ProviderHealthSnapshot {
        self.snapshot_at(Instant::now(), unix_now_ms())
    }

    pub(crate) fn snapshot_at(&self, now: Instant, now_ms: u64) -> ProviderHealthSnapshot {
        snapshot_of(&self.lock(), self.config.open_cooldown, now, now_ms)
    }

    /// Acquires permission for one request, or returns None while the circuit is open.
    pub fn try_acquire(&self) -> Option<RequestPermit<'_>> {
        self.try_acquire_at(Instant::now())
    }

    /// Deterministic-clock variant of try_acquire.
    pub fn try_acquire_at(&self, now: Instant) -> Option<RequestPermit<'_>> {
        let mut inner = self.lock();
        let mut transitioned = false;
        let permit = match inner.state {
            CircuitState::Closed => Some(RequestPermit {
                health: self,
                generation: inner.generation,
                probe: false,
                completed: false,
            }),
            CircuitState::Open { opened_at }
                if cooldown_elapsed(opened_at, now, self.config.open_cooldown) =>
            {
                inner.state = CircuitState::HalfOpen;
                inner.probe_claimed = true;
                transitioned = true;
                Some(RequestPermit {
                    health: self,
                    generation: inner.generation,
                    probe: true,
                    completed: false,
                })
            }
            CircuitState::Open { .. } => None,
            CircuitState::HalfOpen if !inner.probe_claimed => {
                inner.probe_claimed = true;
                Some(RequestPermit {
                    health: self,
                    generation: inner.generation,
                    probe: true,
                    completed: false,
                })
            }
            CircuitState::HalfOpen => None,
        };
        let snapshot = transitioned
            .then(|| snapshot_of(&inner, self.config.open_cooldown, now, unix_now_ms()));
        drop(inner);
        if let Some(snapshot) = snapshot {
            self.persist(snapshot);
        }
        permit
    }

    fn finish(&self, generation: u64, probe: bool, outcome: Outcome, now: Instant) {
        let mut inner = self.lock();
        if inner.generation != generation {
            return;
        }
        if probe {
            if !matches!(inner.state, CircuitState::HalfOpen) {
                return;
            }
            inner.probe_claimed = false;
            match outcome {
                Outcome::Success => {
                    inner.consecutive_successes = inner.consecutive_successes.saturating_add(1);
                    if inner.consecutive_successes >= self.config.success_threshold {
                        inner.state = CircuitState::Closed;
                        inner.consecutive_failures = 0;
                        inner.consecutive_successes = 0;
                        inner.total_requests = 0;
                        inner.failed_requests = 0;
                        inner.generation = inner.generation.wrapping_add(1);
                    }
                }
                Outcome::Failure => open_circuit(&mut inner, now, self.config.failure_threshold),
            }
        } else {
            if !matches!(inner.state, CircuitState::Closed) {
                return;
            }
            inner.total_requests = inner.total_requests.saturating_add(1);
            match outcome {
                Outcome::Success => inner.consecutive_failures = 0,
                Outcome::Failure => {
                    inner.consecutive_failures = inner.consecutive_failures.saturating_add(1);
                    inner.failed_requests = inner.failed_requests.saturating_add(1);
                }
            }
            let rate_open = inner.total_requests >= self.config.min_requests
                && u64::from(inner.failed_requests) * 100
                    >= u64::from(inner.total_requests) * u64::from(self.config.error_rate_percent);
            if inner.consecutive_failures >= self.config.failure_threshold || rate_open {
                open_circuit(&mut inner, now, self.config.failure_threshold);
            }
        }
        let snapshot = snapshot_of(&inner, self.config.open_cooldown, now, unix_now_ms());
        drop(inner);
        self.persist(snapshot);
    }

    fn release(&self, generation: u64, probe: bool, now: Instant) {
        if !probe {
            return;
        }
        let mut inner = self.lock();
        if inner.generation == generation && matches!(inner.state, CircuitState::HalfOpen) {
            inner.state = CircuitState::Open { opened_at: now };
            inner.probe_claimed = false;
            let snapshot = snapshot_of(&inner, self.config.open_cooldown, now, unix_now_ms());
            drop(inner);
            self.persist(snapshot);
        }
    }

    fn persist(&self, snapshot: ProviderHealthSnapshot) {
        if let Some(persistence) = &self.persistence {
            persistence(snapshot);
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for ProviderHealth {
    fn default() -> Self {
        Self::new(ProviderHealthConfig::default())
    }
}

struct Inner {
    state: CircuitState,
    consecutive_failures: u32,
    generation: u64,
    probe_claimed: bool,
    consecutive_successes: u32,
    total_requests: u32,
    failed_requests: u32,
}

impl Inner {
    fn closed() -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
            generation: 0,
            probe_claimed: false,
            consecutive_successes: 0,
            total_requests: 0,
            failed_requests: 0,
        }
    }

    fn from_snapshot(
        snapshot: Option<ProviderHealthSnapshot>,
        cooldown: Duration,
        now: Instant,
        now_ms: u64,
    ) -> Self {
        let Some(snapshot) = snapshot else {
            return Self::closed();
        };
        let state = match snapshot.state {
            ProviderHealthState::Closed => CircuitState::Closed,
            ProviderHealthState::HalfOpen => CircuitState::HalfOpen,
            ProviderHealthState::Open => {
                let Some(open_until_ms) = snapshot.open_until_ms else {
                    return Self {
                        state: CircuitState::HalfOpen,
                        consecutive_failures: snapshot.consecutive_failures,
                        generation: 0,
                        probe_claimed: false,
                        consecutive_successes: snapshot.consecutive_successes,
                        total_requests: snapshot.total_requests,
                        failed_requests: snapshot.failed_requests,
                    };
                };
                if open_until_ms <= now_ms {
                    CircuitState::HalfOpen
                } else {
                    let remaining = Duration::from_millis(open_until_ms - now_ms);
                    let elapsed = cooldown.saturating_sub(remaining);
                    CircuitState::Open {
                        opened_at: now.checked_sub(elapsed).unwrap_or(now),
                    }
                }
            }
        };
        Self {
            state,
            consecutive_failures: snapshot.consecutive_failures,
            generation: 0,
            probe_claimed: false,
            consecutive_successes: snapshot.consecutive_successes,
            total_requests: snapshot.total_requests,
            failed_requests: snapshot.failed_requests,
        }
    }
}

fn open_circuit(inner: &mut Inner, now: Instant, threshold: u32) {
    inner.state = CircuitState::Open { opened_at: now };
    inner.consecutive_failures = inner.consecutive_failures.max(threshold);
    inner.consecutive_successes = 0;
    inner.probe_claimed = false;
    inner.generation = inner.generation.wrapping_add(1);
}

enum CircuitState {
    Closed,
    Open { opened_at: Instant },
    HalfOpen,
}

enum Outcome {
    Success,
    Failure,
}

fn visible_state(inner: &Inner, cooldown: Duration, now: Instant) -> ProviderHealthState {
    match inner.state {
        CircuitState::Closed => ProviderHealthState::Closed,
        CircuitState::HalfOpen => ProviderHealthState::HalfOpen,
        CircuitState::Open { opened_at } => {
            if cooldown_elapsed(opened_at, now, cooldown) {
                ProviderHealthState::HalfOpen
            } else {
                ProviderHealthState::Open
            }
        }
    }
}

fn cooldown_elapsed(opened_at: Instant, now: Instant, cooldown: Duration) -> bool {
    now.checked_duration_since(opened_at)
        .is_some_and(|elapsed| elapsed >= cooldown)
}

fn snapshot_of(
    inner: &Inner,
    cooldown: Duration,
    now: Instant,
    now_ms: u64,
) -> ProviderHealthSnapshot {
    let open_until_ms = match inner.state {
        CircuitState::Open { opened_at } => now
            .checked_duration_since(opened_at)
            .map(|elapsed| cooldown.saturating_sub(elapsed))
            .map(|remaining| now_ms.saturating_add(remaining.as_millis() as u64)),
        CircuitState::Closed | CircuitState::HalfOpen => None,
    };
    ProviderHealthSnapshot {
        state: match inner.state {
            CircuitState::Closed => ProviderHealthState::Closed,
            CircuitState::Open { .. } => ProviderHealthState::Open,
            CircuitState::HalfOpen => ProviderHealthState::HalfOpen,
        },
        consecutive_failures: inner.consecutive_failures,
        consecutive_successes: inner.consecutive_successes,
        total_requests: inner.total_requests,
        failed_requests: inner.failed_requests,
        open_until_ms,
    }
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "provider_health_tests.rs"]
mod tests;
