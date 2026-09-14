use super::*;
use std::sync::{Arc, Barrier};
use std::thread;

fn config(threshold: u32, cooldown: Duration) -> ProviderHealthConfig {
    ProviderHealthConfig::new(threshold, cooldown)
}

#[test]
fn transitions_closed_open_half_open_and_closed() {
    let start = Instant::now();
    let health = ProviderHealth::new(config(2, Duration::from_secs(10)));

    health
        .try_acquire_at(start)
        .unwrap()
        .record_failure_at(start);
    assert_eq!(health.state_at(start), ProviderHealthState::Closed);
    assert_eq!(health.consecutive_failures(), 1);

    health
        .try_acquire_at(start)
        .unwrap()
        .record_failure_at(start);
    assert_eq!(health.state_at(start), ProviderHealthState::Open);
    assert!(health
        .try_acquire_at(start + Duration::from_secs(9))
        .is_none());
    assert_eq!(
        health.state_at(start + Duration::from_secs(10)),
        ProviderHealthState::HalfOpen
    );

    let probe = health
        .try_acquire_at(start + Duration::from_secs(10))
        .unwrap();
    assert!(probe.is_probe());
    assert!(health
        .try_acquire_at(start + Duration::from_secs(10))
        .is_none());
    probe.record_success_at(start + Duration::from_secs(10));
    assert_eq!(health.state(), ProviderHealthState::Closed);
    assert_eq!(health.consecutive_failures(), 0);
}

#[test]
fn half_open_failure_reopens_circuit_and_drop_releases_probe() {
    let start = Instant::now();
    let health = ProviderHealth::new(config(1, Duration::from_secs(5)));
    health
        .try_acquire_at(start)
        .unwrap()
        .record_failure_at(start);

    let probe = health
        .try_acquire_at(start + Duration::from_secs(5))
        .unwrap();
    drop(probe);
    assert_eq!(health.state(), ProviderHealthState::Open);
    assert!(health.try_acquire().is_none());

    let start = Instant::now();
    let health = ProviderHealth::new(config(1, Duration::from_secs(5)));
    health
        .try_acquire_at(start)
        .unwrap()
        .record_failure_at(start);
    let probe = health
        .try_acquire_at(start + Duration::from_secs(10))
        .unwrap();
    probe.record_failure_at(start + Duration::from_secs(10));
    assert_eq!(
        health.state_at(start + Duration::from_secs(10)),
        ProviderHealthState::Open
    );
    assert!(health
        .try_acquire_at(start + Duration::from_secs(14))
        .is_none());
}

#[test]
fn stale_closed_request_cannot_close_new_open_generation() {
    let start = Instant::now();
    let health = ProviderHealth::new(config(1, Duration::from_secs(10)));
    let first = health.try_acquire_at(start).unwrap();
    let second = health.try_acquire_at(start).unwrap();
    first.record_failure_at(start);
    second.record_success_at(start + Duration::from_secs(1));
    assert_eq!(
        health.state_at(start + Duration::from_secs(1)),
        ProviderHealthState::Open
    );
}

#[test]
fn only_one_thread_can_claim_half_open_probe() {
    let start = Instant::now();
    let health = Arc::new(ProviderHealth::new(config(1, Duration::ZERO)));
    health
        .try_acquire_at(start)
        .unwrap()
        .record_failure_at(start);

    let workers = 32;
    let barrier = Arc::new(Barrier::new(workers));
    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let health = Arc::clone(&health);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            let permit = health.try_acquire_at(start);
            let claimed = permit.is_some();
            barrier.wait();
            if let Some(permit) = permit {
                permit.record_success_at(start);
            }
            claimed
        }));
    }

    let claimed = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .filter(|claimed| *claimed)
        .count();
    assert_eq!(claimed, 1);
    assert_eq!(health.state_at(start), ProviderHealthState::Closed);
}

#[test]
fn retry_classification_separates_provider_errors_from_client_errors() {
    for status in [401, 403, 404, 408, 409, 429, 451, 500, 503, 599] {
        assert!(is_retryable_http_status(status));
    }
    for status in [200, 400, 405, 406, 413, 414, 415, 422, 499, 501, 600] {
        assert!(!is_retryable_http_status(status));
    }
}

#[test]
fn error_rate_and_multiple_half_open_successes_are_configurable() {
    let mut config = config(100, Duration::from_secs(5));
    config.min_requests = 4;
    config.error_rate_percent = 50;
    config.success_threshold = 2;
    let health = ProviderHealth::new(config);
    let now = Instant::now();
    health.try_acquire_at(now).unwrap().record_failure_at(now);
    health.try_acquire_at(now).unwrap().record_success_at(now);
    health.try_acquire_at(now).unwrap().record_failure_at(now);
    health.try_acquire_at(now).unwrap().record_success_at(now);
    assert_eq!(health.state_at(now), ProviderHealthState::Open);
    let later = now + Duration::from_secs(6);
    health
        .try_acquire_at(later)
        .unwrap()
        .record_success_at(later);
    assert_eq!(health.state_at(later), ProviderHealthState::HalfOpen);
    health
        .try_acquire_at(later)
        .unwrap()
        .record_success_at(later);
    assert_eq!(health.state_at(later), ProviderHealthState::Closed);
}
