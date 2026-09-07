use super::ledger::{
    prune_providers, save, OfficialHistoryPoint, ProviderHistoryPoint, UsageHistoryLedger,
    HISTORY_RETENTION_DAYS, MAX_POINTS_PER_SERIES,
};
use super::*;
use crate::local_state::LocalState;
use asb_core::contracts::{
    AppKind, CodexOfficialQuota, CodexOfficialQuotaStatus, CodexOfficialQuotaWindow, ProviderDraft,
    ProviderProfile, RouteMode, UpstreamProtocol, UsageHistoryMetric, UsageHistoryPoint,
    UsageQuery, UsageReading, UsageSummary,
};
use chrono::Duration;
use chrono::Utc;
use std::fs;
use tempfile::tempdir;

fn provider() -> ProviderProfile {
    ProviderProfile::from_draft(
        "provider-1".to_string(),
        ProviderDraft {
            app: AppKind::Codex,
            route_mode: RouteMode::Custom,
            name: "示例中转".to_string(),
            model: None,
            base_url: Some("https://relay.example".to_string()),
            api_key: "test-api-key".to_string(),
            upstream_protocol: Some(UpstreamProtocol::Responses),
            max_output_tokens: None.into(),
            model_options: None,
            notes: None,
            website_url: None,
            usage_query: Some(UsageQuery::Script {
                source: "({ request() {}, extract() {} })".to_string(),
                refresh_interval_minutes: 0,
            }),
            official_quota_refresh_interval_minutes: None,
        },
    )
}

fn summary(at: &str) -> UsageSummary {
    UsageSummary {
        at: at.to_string(),
        readings: vec![UsageReading {
            plan_name: Some("专业版".to_string()),
            remaining: Some(70.0),
            used: None,
            total: Some(100.0),
            unit: Some("次".to_string()),
        }],
    }
}

fn quota(at: &str, used_percent: f64) -> CodexOfficialQuota {
    CodexOfficialQuota {
        status: CodexOfficialQuotaStatus::Available,
        windows: vec![CodexOfficialQuotaWindow {
            label: "7 天".to_string(),
            used_percent,
            resets_at: Some("2026-09-10T00:00:00Z".to_string()),
        }],
        at: Some(at.to_string()),
        stale: false,
        last_reset: None,
    }
}

#[test]
fn records_provider_numbers_without_persisting_profile_secrets_or_query_source() {
    let directory = tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = provider();

    record_provider(&state, &profile, &summary("2026-09-03T08:00:00Z"))
        .expect("record provider history");

    let persisted = fs::read_to_string(state.usage_history_path()).expect("history file");
    assert!(!persisted.contains("test-api-key"));
    assert!(!persisted.contains("relay.example"));
    assert!(!persisted.contains("extract()"));

    let series = provider_series(&state, &profile).expect("provider series");
    assert_eq!(series.len(), 2);
    assert!(series.iter().any(|series| {
        series.metric == UsageHistoryMetric::Remaining
            && series.points[0].value == 70.0
            && series.unit.as_deref() == Some("次")
    }));
    assert!(series.iter().any(|series| {
        series.metric == UsageHistoryMetric::Used && series.points[0].value == 30.0
    }));
    assert!(series
        .iter()
        .all(|series| series.metric != UsageHistoryMetric::UsedPercent));
}

#[test]
fn successive_provider_reads_append_to_the_same_trend() {
    let directory = tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = provider();

    record_provider(&state, &profile, &summary("2026-09-03T08:00:00Z"))
        .expect("record first provider history");
    record_provider(&state, &profile, &summary("2026-09-03T09:00:00Z"))
        .expect("record second provider history");

    let remaining = provider_series(&state, &profile)
        .expect("provider series")
        .into_iter()
        .find(|series| series.metric == UsageHistoryMetric::Remaining)
        .expect("remaining series");
    assert_eq!(remaining.points.len(), 2);
    assert_eq!(remaining.points[1].at, "2026-09-03T09:00:00.000Z");
}

#[test]
fn only_the_current_provider_query_digest_can_read_its_history() {
    let directory = tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = provider();
    record_provider(&state, &profile, &summary("2026-09-03T08:00:00Z"))
        .expect("record provider history");
    let mut changed = profile.clone();
    changed.usage_query = Some(UsageQuery::Script {
        source: "({ request() { return {}; }, extract() { return {}; } })".to_string(),
        refresh_interval_minutes: 0,
    });

    assert!(provider_series(&state, &changed)
        .expect("changed query series")
        .is_empty());
}

#[test]
fn invalidating_one_profile_and_resetting_the_store_remove_provider_history() {
    let directory = tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = provider();
    record_provider(&state, &profile, &summary("2026-09-03T08:00:00Z"))
        .expect("record provider history");

    invalidate_provider(&state, &profile.id).expect("invalidate profile history");
    assert!(provider_series(&state, &profile)
        .expect("invalidated profile series")
        .is_empty());
    assert!(!state.usage_history_path().exists());

    record_provider(&state, &profile, &summary("2026-09-03T09:00:00Z"))
        .expect("record provider history again");
    record_official(&state, &quota("2026-09-03T09:00:00Z", 20.0), false)
        .expect("record independent official history");

    clear_providers(&state).expect("reset provider history");

    assert!(provider_series(&state, &profile)
        .expect("cleared provider series")
        .is_empty());
    assert_eq!(
        official_series(&state)
            .expect("official history remains")
            .len(),
        1
    );
}

#[test]
fn pruning_drops_points_older_than_the_365_day_retention_window() {
    let recent = Utc::now().to_rfc3339();
    let expired = (Utc::now() - Duration::days(HISTORY_RETENTION_DAYS + 1)).to_rfc3339();
    let mut points = vec![
        ProviderHistoryPoint {
            profile_id: "profile".to_string(),
            query_digest: "digest".to_string(),
            at: expired,
            plan_name: None,
            unit: None,
            remaining: Some(1.0),
            used: None,
            total: None,
        },
        ProviderHistoryPoint {
            profile_id: "profile".to_string(),
            query_digest: "digest".to_string(),
            at: recent,
            plan_name: None,
            unit: None,
            remaining: Some(2.0),
            used: None,
            total: None,
        },
    ];

    prune_providers(&mut points);

    assert_eq!(points.len(), 1);
    assert_eq!(points[0].remaining, Some(2.0));
}

#[test]
fn reading_history_hides_expired_snapshots_without_a_new_successful_read() {
    let directory = tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = provider();
    let digest =
        crate::usage_cache::query_digest(profile.usage_query.as_ref().expect("usage query"))
            .expect("query digest");
    let expired = (Utc::now() - Duration::days(HISTORY_RETENTION_DAYS + 1)).to_rfc3339();
    let ledger = UsageHistoryLedger {
        providers: vec![ProviderHistoryPoint {
            profile_id: profile.id.clone(),
            query_digest: digest,
            at: expired.clone(),
            plan_name: Some("专业版".to_string()),
            unit: Some("次".to_string()),
            remaining: Some(70.0),
            used: None,
            total: Some(100.0),
        }],
        official: vec![OfficialHistoryPoint {
            at: expired,
            window_label: "7 天".to_string(),
            used_percent: 20.0,
            resets_at: None,
        }],
    };
    save(&state, &ledger).expect("persist expired history");

    assert!(provider_series(&state, &profile)
        .expect("provider history")
        .is_empty());
    assert!(official_series(&state)
        .expect("official history")
        .is_empty());
}

#[test]
fn detected_official_account_change_replaces_the_official_trend() {
    let directory = tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    record_official(&state, &quota("2026-09-03T08:00:00Z", 20.0), false)
        .expect("record first account");
    record_official(&state, &quota("2026-09-03T09:00:00Z", 40.0), true)
        .expect("record changed account");

    let series = official_series(&state).expect("official series");
    assert_eq!(series.len(), 1);
    assert_eq!(
        series[0].points,
        vec![UsageHistoryPoint {
            at: "2026-09-03T09:00:00.000Z".to_string(),
            value: 40.0,
        }]
    );
}

#[test]
fn malformed_history_is_rejected_without_rewriting_it() {
    let directory = tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    fs::create_dir_all(state.usage_history_path().parent().expect("parent"))
        .expect("state directory");
    let malformed = r#"{"providers":[{"profileId":"p"}],"official":[]}"#;
    fs::write(state.usage_history_path(), malformed).expect("malformed history");

    assert_eq!(
        record_provider(&state, &provider(), &summary("2026-09-03T08:00:00Z")).unwrap_err(),
        "用量历史格式无效"
    );
    assert_eq!(
        fs::read_to_string(state.usage_history_path()).expect("original history"),
        malformed
    );
}

#[test]
fn pruning_keeps_the_newest_720_points_in_each_provider_series() {
    let recent = Utc::now();
    let mut points = (0..722)
        .map(|index| ProviderHistoryPoint {
            profile_id: "profile".to_string(),
            query_digest: "digest".to_string(),
            at: (recent - Duration::milliseconds((721 - index) as i64)).to_rfc3339(),
            plan_name: None,
            unit: None,
            remaining: Some(index as f64),
            used: None,
            total: None,
        })
        .collect::<Vec<_>>();

    prune_providers(&mut points);

    assert_eq!(points.len(), MAX_POINTS_PER_SERIES);
    assert_eq!(points[0].remaining, Some(2.0));
    assert_eq!(points.last().and_then(|point| point.remaining), Some(721.0));
}
