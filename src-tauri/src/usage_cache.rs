//! Credential-free usage snapshots for the custom tray panel.
//!
//! The desktop scheduler owns when a profile is re-queried; this cache is the
//! single persisted record of that timing. A successful provider query
//! replaces its prior snapshot atomically. The snapshot stores only the
//! normalized readings, the digest of the profile's current query, and the
//! last attempt time — never a credential, endpoint, raw response, or script.

use crate::local_state::LocalState;
use asb_core::contracts::{ProviderProfile, UsageQuery, UsageSummary};
use asb_switch::sha256_hex;
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Serializes every read-modify-write of the cache file so the scheduler
/// thread and a manual query can never lose each other's entries.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UsageCache {
    entries: BTreeMap<String, CachedUsage>,
}

impl Default for UsageCache {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CachedUsage {
    query_digest: String,
    /// RFC 3339 UTC timestamp of the most recent attempt, successful or not.
    /// Failures advance it too, so a failing endpoint is retried on its own
    /// configured cadence instead of every scheduler tick.
    attempted_at: String,
    /// The last successful summary; absent once every attempt so far failed
    /// or the profile's query changed since that success.
    summary: Option<UsageSummary>,
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// The stable digest for every persisted representation of a usage query.
/// It is intentionally one-way because the source can mention private URLs
/// or contain script text that must not enter display-oriented state.
pub(crate) fn query_digest(query: &UsageQuery) -> Result<String, String> {
    let serialized =
        serde_json::to_string(query).map_err(|_| "用量查询摘要序列化失败".to_string())?;
    Ok(sha256_hex(&serialized))
}

/// Returns the last successful reading only while it belongs to the
/// profile's current query. A profile edit never displays a stale result.
pub(crate) fn get(state: &LocalState, profile: &ProviderProfile) -> Option<UsageSummary> {
    let query = profile.usage_query.as_ref()?;
    let cache = state.load_usage_cache().ok()??;
    let digest = query_digest(query).ok()?;
    cache
        .entries
        .get(&profile.id)
        .filter(|cached| cached.query_digest == digest)
        .and_then(|cached| cached.summary.clone())
}

/// Replaces the snapshot for one profile and stamps the attempt time after a
/// successful real query.
pub(crate) fn record_success(
    state: &LocalState,
    profile: &ProviderProfile,
    summary: UsageSummary,
) -> Result<(), String> {
    let query = profile
        .usage_query
        .as_ref()
        .ok_or_else(|| "该供应商尚未配置用量查询".to_string())?;
    let digest = query_digest(query)?;
    let _guard = WRITE_LOCK.lock().map_err(|_| "用量缓存写入锁不可用")?;
    let mut cache = state.load_usage_cache()?.unwrap_or_default();
    cache.entries.insert(
        profile.id.clone(),
        CachedUsage {
            query_digest: digest,
            attempted_at: now_rfc3339(),
            summary: Some(summary),
        },
    );
    state.save_usage_cache(&cache)
}

/// Stamps the attempt time after a failed query, keeping the last successful
/// summary only while it still belongs to the profile's current query.
pub(crate) fn record_failure(state: &LocalState, profile: &ProviderProfile) -> Result<(), String> {
    let Some(query) = profile.usage_query.as_ref() else {
        return Ok(());
    };
    let digest = query_digest(query)?;
    let _guard = WRITE_LOCK.lock().map_err(|_| "用量缓存写入锁不可用")?;
    let mut cache = state.load_usage_cache()?.unwrap_or_default();
    let retained = cache
        .entries
        .get(&profile.id)
        .filter(|cached| cached.query_digest == digest)
        .and_then(|cached| cached.summary.clone());
    cache.entries.insert(
        profile.id.clone(),
        CachedUsage {
            query_digest: digest,
            attempted_at: now_rfc3339(),
            summary: retained,
        },
    );
    state.save_usage_cache(&cache)
}

/// Whether the scheduler should query this profile now: an absent or
/// unparsable attempt time, one recorded for a different query, or one older
/// than the configured interval is due.
pub(crate) fn due(
    state: &LocalState,
    profile: &ProviderProfile,
    interval_minutes: u32,
    now: DateTime<Utc>,
) -> bool {
    let Some(query) = profile.usage_query.as_ref() else {
        return false;
    };
    let digest = query_digest(query).ok();
    let cache = state.load_usage_cache().ok().flatten();
    let attempted_at = cache
        .as_ref()
        .and_then(|cache| cache.entries.get(&profile.id))
        .filter(|cached| Some(&cached.query_digest) == digest.as_ref())
        .map(|cached| cached.attempted_at.as_str());
    attempt_is_due(attempted_at, interval_minutes, now)
}

/// The pure timing rule behind `due`, separated for direct testing.
pub(crate) fn attempt_is_due(
    attempted_at: Option<&str>,
    interval_minutes: u32,
    now: DateTime<Utc>,
) -> bool {
    let Some(attempted_at) = attempted_at
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.with_timezone(&Utc))
    else {
        return true;
    };
    now.signed_duration_since(attempted_at).num_seconds() >= i64::from(interval_minutes) * 60
}

/// Removes the snapshot after its profile was changed or deleted.
pub(crate) fn invalidate(state: &LocalState, profile_id: &str) -> Result<(), String> {
    let _guard = WRITE_LOCK.lock().map_err(|_| "用量缓存写入锁不可用")?;
    let Some(mut cache) = state.load_usage_cache()? else {
        return Ok(());
    };
    if cache.entries.remove(profile_id).is_some() {
        state.save_usage_cache(&cache)?;
    }
    Ok(())
}

/// Drops all snapshots when the application-owned profile store is reset.
pub(crate) fn clear(state: &LocalState) -> Result<(), String> {
    let _guard = WRITE_LOCK.lock().map_err(|_| "用量缓存写入锁不可用")?;
    state.clear_usage_cache()
}

#[cfg(test)]
mod tests {
    use super::*;
    use asb_core::contracts::{AppKind, ProviderDraft, RouteMode, UpstreamProtocol, UsageReading};
    use std::fs;

    fn profile() -> ProviderProfile {
        ProviderProfile::from_draft(
            "profile-1".to_string(),
            ProviderDraft {
                app: AppKind::Claude,
                route_mode: RouteMode::Custom,
                name: "查询供应商".to_string(),
                model: None,
                base_url: Some("https://relay.example".to_string()),
                api_key: "test-api-key".to_string(),
                upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
                max_output_tokens: None.into(),
                model_options: None,
                notes: None,
                website_url: None,
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
            }],
            at: "2026-09-02T02:34:00Z".to_string(),
        }
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
        )
        .expect("record success");

        let persisted = fs::read_to_string(root.join("usage-cache.json")).expect("cache text");
        assert!(!persisted.contains("test-api-key"));
        assert!(!persisted.contains("relay.example"));
        assert!(!persisted.contains("{{baseUrl}}/usage"));

        assert_eq!(get(&LocalState::from_root(root), &profile), Some(expected));
    }

    #[test]
    fn a_cache_file_from_a_prior_format_is_dropped_and_rebuilt_by_the_next_query() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let root = directory.path().join("state");
        let state = LocalState::from_root(root.clone());
        fs::create_dir_all(&root).expect("create state directory");
        let legacy = r#"{"entries":{"profile-1":{"queryDigest":"stale","summary":{"readings":[],"at":"2026-09-06T00:00:00Z"}}}}"#;
        fs::write(root.join("usage-cache.json"), legacy).expect("write legacy cache");

        record_success(&state, &profile(), summary()).expect("rebuild cache");

        let persisted = fs::read_to_string(root.join("usage-cache.json")).expect("rebuilt cache");
        assert!(persisted.contains("attemptedAt"));
        assert_eq!(get(&state, &profile()), Some(summary()));
    }

    #[test]
    fn a_changed_query_hides_its_prior_summary() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let state = LocalState::from_root(directory.path().join("state"));
        let profile = profile();
        record_success(&state, &profile, summary()).expect("record success");
        let mut changed = profile.clone();
        changed.usage_query = Some(UsageQuery::Declarative {
            url: "{{baseUrl}}/new-usage".to_string(),
            remaining_path: Some("/remaining".to_string()),
            used_path: None,
            total_path: None,
            unit: Some("次".to_string()),
            refresh_interval_minutes: 0,
        });

        assert_eq!(get(&state, &changed), None);
    }

    #[test]
    fn a_failed_attempt_keeps_the_last_matching_summary_but_advances_timing() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let state = LocalState::from_root(directory.path().join("state"));
        let profile = profile();
        record_success(&state, &profile, summary()).expect("record success");

        record_failure(&state, &profile).expect("record failure");

        assert_eq!(get(&state, &profile), Some(summary()));
        let now = Utc::now();
        assert!(!due(&state, &profile, 30, now));
    }

    #[test]
    fn a_failed_first_attempt_displays_nothing() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let state = LocalState::from_root(directory.path().join("state"));
        let profile = profile();

        record_failure(&state, &profile).expect("record failure");

        assert_eq!(get(&state, &profile), None);
    }

    #[test]
    fn clearing_the_profile_store_cache_removes_the_snapshot_file() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let root = directory.path().join("state");
        let state = LocalState::from_root(root.clone());
        let profile = profile();
        record_success(&state, &profile, summary()).expect("record success");

        clear(&state).expect("clear cache");

        assert!(!root.join("usage-cache.json").exists());
        assert_eq!(get(&state, &profile), None);
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

        record_success(&state, &profile, summary()).expect("record success");
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
}
