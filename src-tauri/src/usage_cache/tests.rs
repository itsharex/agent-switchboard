use super::*;
use asb_core::contracts::{AppKind, ProviderDraft, RouteMode, UpstreamProtocol, UsageReading};
use std::fs;

fn profile() -> ProviderProfile {
    ProviderProfile::from_draft(
        "profile-1".to_string(),
        ProviderDraft {
            authentication: None,
            parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
            claude_fragment: Default::default(),
            app: AppKind::Claude,
            route_mode: RouteMode::Custom,
            name: "查询供应商".to_string(),
            model: None,
            base_url: Some("https://relay.example".to_string()),
            connection: Default::default(),
            api_key: "test-api-key".to_string(),
            upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
            responses_options: None,
            max_output_tokens: None.into(),
            model_options: None,
            notes: None,
            website_url: None,
            display: None,
            usage_query: Some(UsageQuery::Declarative {
                url: "{{baseUrl}}/usage".to_string(),
                remaining_path: Some("/remaining".to_string()),
                used_path: None,
                total_path: None,
                unit: Some("次".to_string()),
                refresh_interval_minutes: 0,
            }),
            official_quota_refresh_interval_minutes: None,
        },
    )
}

fn summary() -> UsageSummary {
    UsageSummary {
        readings: vec![UsageReading {
            plan_name: Some("额度".to_string()),
            remaining: Some(92.0),
            used: Some(8.0),
            total: Some(100.0),
            unit: Some("%".to_string()),
            resets_at: None,
            is_valid: None,
            invalid_message: None,
            extra: None,
        }],
        at: "2026-09-02T02:34:00Z".to_string(),
    }
}

/// A fixed initiation instant, matching the summary's own timestamp.
fn attempt_time() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-09-02T02:34:00Z")
        .expect("attempt time")
        .with_timezone(&Utc)
}

#[test]
fn last_successful_summary_survives_a_state_reopen() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().join("state");
    let profile = profile();
    let expected = summary();

    record_success(
        &LocalState::from_root(root.clone()),
        &profile,
        expected.clone(),
        attempt_time(),
    )
    .expect("record success");

    let persisted = fs::read_to_string(root.join("usage-cache.json")).expect("cache text");
    assert!(persisted.contains("2026-09-02T02:34:00.000000000Z"));
    assert!(!persisted.contains("test-api-key"));
    assert!(!persisted.contains("relay.example"));
    assert!(!persisted.contains("{{baseUrl}}/usage"));

    assert_eq!(get(&LocalState::from_root(root), &profile), Ok(Some(UsageSnapshot { summary: Some(expected), error: None })));
}

#[test]
fn a_cache_file_from_a_prior_format_is_dropped_and_rebuilt_by_the_next_query() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().join("state");
    let state = LocalState::from_root(root.clone());
    fs::create_dir_all(&root).expect("create state directory");
    let legacy = r#"{"entries":{"profile-1":{"queryDigest":"stale","summary":{"readings":[],"at":"2026-09-06T00:00:00Z"}}}}"#;
    fs::write(root.join("usage-cache.json"), legacy).expect("write legacy cache");

    record_success(&state, &profile(), summary(), attempt_time()).expect("rebuild cache");

    let persisted = fs::read_to_string(root.join("usage-cache.json")).expect("rebuilt cache");
    assert!(persisted.contains("attemptedAt"));
    assert_eq!(get(&state, &profile()), Ok(Some(UsageSnapshot { summary: Some(summary()), error: None })));
}

#[test]
fn a_changed_query_hides_its_prior_summary() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = profile();
    record_success(&state, &profile, summary(), attempt_time()).expect("record success");
    let mut changed = profile.clone();
    changed.usage_query = Some(UsageQuery::Declarative {
        url: "{{baseUrl}}/new-usage".to_string(),
        remaining_path: Some("/remaining".to_string()),
        used_path: None,
        total_path: None,
        unit: Some("次".to_string()),
        refresh_interval_minutes: 0,
    });

    assert_eq!(get(&state, &changed), Ok(None));
}

#[test]
fn a_failed_attempt_keeps_the_last_matching_summary_but_advances_timing() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = profile();
    let attempted_at = Utc::now();
    record_success(&state, &profile, summary(), attempted_at).expect("record success");

    record_failure(&state, &profile, attempted_at, "查询失败").expect("record failure");

    assert_eq!(get(&state, &profile), Ok(Some(UsageSnapshot {
        summary: Some(summary()), error: Some("查询失败".into()),
    })));
    let now = Utc::now();
    assert!(!due(&state, &profile, 30, now));
    assert!(due(
        &state,
        &profile,
        30,
        attempted_at + chrono::Duration::seconds(30 * 60)
    ));
}

#[test]
fn a_failed_first_attempt_displays_the_error_without_a_balance() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = profile();

    record_failure(&state, &profile, Utc::now(), "查询失败").expect("record failure");

    assert_eq!(get(&state, &profile), Ok(Some(UsageSnapshot {
        summary: None, error: Some("查询失败".into()),
    })));
}

#[test]
fn late_success_or_failure_cannot_replace_a_newer_attempt() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = profile();
    let old = attempt_time();
    let recent = old + chrono::Duration::nanoseconds(1);
    let mut latest = summary();
    latest.readings[0].remaining = Some(50.0);
    assert!(record_success(&state, &profile, latest.clone(), recent).unwrap());
    assert!(!record_success(&state, &profile, summary(), old).unwrap());
    record_failure(&state, &profile, old, "查询失败").unwrap();
    assert_eq!(get(&state, &profile), Ok(Some(UsageSnapshot { summary: Some(latest), error: None })));
    let cache = state.load_usage_cache().unwrap().unwrap();
    assert_eq!(cache.entries[&profile.id].attempted_at, rfc3339(recent));
}

#[test]
fn late_success_cannot_undo_a_more_recent_failure() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = profile();
    let old = attempt_time();
    let recent = old + chrono::Duration::seconds(1);
    record_failure(&state, &profile, recent, "查询失败").unwrap();
    assert!(!record_success(&state, &profile, summary(), old).unwrap());
    assert_eq!(get(&state, &profile), Ok(Some(UsageSnapshot {
        summary: None, error: Some("查询失败".into()),
    })));
    assert!(!due(&state, &profile, 1, recent));
}

#[test]
fn clearing_the_profile_store_cache_removes_the_snapshot_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().join("state");
    let state = LocalState::from_root(root.clone());
    let profile = profile();
    record_success(&state, &profile, summary(), attempt_time()).expect("record success");

    clear(&state).expect("clear cache");

    assert!(!root.join("usage-cache.json").exists());
    assert_eq!(get(&state, &profile), Ok(None));
}

#[test]
fn an_attempt_becomes_due_after_its_interval_elapses() {
    let now = DateTime::parse_from_rfc3339("2026-09-07T10:00:00Z")
        .expect("now")
        .with_timezone(&Utc);
    assert!(attempt_is_due(None, 30, now));
    assert!(attempt_is_due(Some("not-a-timestamp"), 30, now));
    assert!(!attempt_is_due(Some("2026-09-07T09:40:00Z"), 30, now));
    assert!(attempt_is_due(Some("2026-09-07T09:29:59Z"), 30, now));
    assert!(attempt_is_due(Some("2026-09-07T09:30:00Z"), 30, now));
}

#[test]
fn a_profile_without_a_query_or_with_a_stale_digest_is_not_schedulable() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = profile();
    let mut unconfigured = profile.clone();
    unconfigured.usage_query = None;
    let now = Utc::now();

    assert!(!due(&state, &unconfigured, 30, now));
    assert!(due(&state, &profile, 30, now));

    record_success(&state, &profile, summary(), attempt_time()).expect("record success");
    let mut changed = profile.clone();
    changed.usage_query = Some(UsageQuery::Declarative {
        url: "{{baseUrl}}/new-usage".to_string(),
        remaining_path: Some("/remaining".to_string()),
        used_path: None,
        total_path: None,
        unit: Some("次".to_string()),
        refresh_interval_minutes: 0,
    });

    assert!(due(&state, &changed, 30, now));
}

#[test]
fn a_success_clears_failure_and_failed_queries_never_persist_the_profile_key() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = profile();
    record_success(&state, &profile, summary(), attempt_time()).unwrap();
    let failed_at = attempt_time() + chrono::Duration::seconds(1);
    record_failure(&state, &profile, failed_at, "拒绝 test-api-key").unwrap();
    let snapshot = get(&state, &profile).unwrap().unwrap();
    assert_eq!(snapshot.summary, Some(summary()));
    assert!(!snapshot.error.unwrap().contains("test-api-key"));
    record_success(&state, &profile, summary(), failed_at + chrono::Duration::seconds(1)).unwrap();
    assert!(get(&state, &profile).unwrap().unwrap().error.is_none());
}
