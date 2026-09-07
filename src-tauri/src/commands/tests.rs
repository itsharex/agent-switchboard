use super::quota::record_official_reset_read;
use crate::local_state::LocalState;
use asb_core::contracts::{CodexOfficialQuota, CodexOfficialQuotaStatus, CodexOfficialQuotaWindow};
use tempfile::tempdir;

fn quota(at: &str, used_percent: f64) -> CodexOfficialQuota {
    CodexOfficialQuota {
        status: CodexOfficialQuotaStatus::Available,
        windows: vec![CodexOfficialQuotaWindow {
            label: "7 天".to_string(),
            used_percent,
            resets_at: None,
        }],
        at: Some(at.to_string()),
        stale: false,
        last_reset: None,
    }
}

#[test]
fn missing_quota_baseline_replaces_the_existing_official_trend() {
    let directory = tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    crate::usage_history::record_official(&state, &quota("2026-09-03T08:00:00Z", 20.0), false)
        .expect("record old account trend");

    record_official_reset_read(
        &state,
        Some("new-account-marker".to_string()),
        &quota("2026-09-03T09:00:00Z", 40.0),
    );

    let series = crate::usage_history::official_series(&state).expect("official trend");
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].points.len(), 1);
    assert_eq!(series[0].points[0].value, 40.0);
}
