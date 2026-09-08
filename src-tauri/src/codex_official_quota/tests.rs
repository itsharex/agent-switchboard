use super::baseline::{apply_read, CodexQuotaBaseline, WEEKLY_WINDOW_LABEL};
use super::service::{
    account_marker, empty, fetch, matching_last_success, parse_credentials,
    quota_from_http_response, quota_window_label, retain_last_success, CachedQuota,
};
use asb_core::contracts::{
    CodexOfficialQuota, CodexOfficialQuotaReset, CodexOfficialQuotaResetKind,
    CodexOfficialQuotaStatus, CodexOfficialQuotaWindow,
};
use std::collections::HashMap;

#[test]
fn accepts_only_the_existing_chatgpt_oauth_shape() {
    let credentials = parse_credentials(
        r#"{"auth_mode":"chatgpt","tokens":{"access_token":"token","refresh_token":"refresh","id_token":"id","account_id":"account"}}"#,
    )
    .expect("OAuth credentials");
    assert_eq!(credentials.access_token, "token");
    assert_eq!(credentials.account_id.as_deref(), Some("account"));
    assert!(parse_credentials(r#"{"auth_mode":"api","tokens":{"access_token":"key"}}"#).is_none());
    assert!(parse_credentials(
        r#"{"auth_mode":"chatgpt","tokens":{"access_token":"line\nbreak"}}"#
    )
    .is_none());
}

#[test]
fn accepts_the_cache_written_by_the_official_login_flow() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let auth_path = directory.path().join("auth.json");
    crate::official_login::credentials::write_codex_auth(
        &auth_path,
        &crate::official_login::credentials::CodexTokens {
            id_token: Some("id-token".to_string()),
            access_token: "access-token".to_string(),
            refresh_token: "refresh-token".to_string(),
        },
        Some("account-1"),
    )
    .expect("official login writes the Codex cache");

    let credentials =
        parse_credentials(&std::fs::read_to_string(&auth_path).expect("written cache"))
            .expect("quota parser accepts the official-login shape");
    assert_eq!(credentials.access_token, "access-token");
    assert_eq!(credentials.account_id.as_deref(), Some("account-1"));
}

#[test]
fn missing_auth_file_requires_sign_in_without_creating_it() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let auth_path = directory.path().join("missing").join("auth.json");

    let (quota, marker) = fetch(&auth_path);

    assert_eq!(quota.status, CodexOfficialQuotaStatus::SignInRequired);
    assert!(quota.windows.is_empty());
    assert_eq!(marker, None);
    assert!(!auth_path.exists());
}

#[test]
fn normalizes_the_two_server_windows_without_credential_data() {
    let quota = quota_from_http_response(
        200,
        r#"{
                "rate_limit": {
                    "primary_window": {"used_percent": 12.5, "limit_window_seconds": 18000, "reset_at": 1760000000},
                    "secondary_window": {"used_percent": 76.25, "limit_window_seconds": 604800, "reset_at": 1760500000}
                }
            }"#,
        "2026-09-01T00:00:00.000Z".to_string(),
    );
    assert_eq!(quota.status, CodexOfficialQuotaStatus::Available);
    assert!(!quota.stale);
    assert_eq!(quota.windows.len(), 2);
    assert_eq!(quota.windows[0].label, "5 小时");
    assert_eq!(quota.windows[0].used_percent, 12.5);
    assert_eq!(quota.windows[1].label, "7 天");
    assert_eq!(quota.windows[1].used_percent, 76.25);
    assert_eq!(quota.at.as_deref(), Some("2026-09-01T00:00:00.000Z"));
}

#[test]
fn rejects_unusable_quota_payloads_and_maps_authorization_failures() {
    let malformed = quota_from_http_response(200, r#"{"rate_limit":{}}"#, "now".to_string());
    assert_eq!(malformed.status, CodexOfficialQuotaStatus::Unavailable);
    assert!(malformed.windows.is_empty());
    assert_eq!(
        quota_from_http_response(401, "{}", "now".to_string()).status,
        CodexOfficialQuotaStatus::ReauthenticationRequired
    );
    assert_eq!(
        quota_from_http_response(503, "{}", "now".to_string()).status,
        CodexOfficialQuotaStatus::Unavailable
    );
}

#[test]
fn keeps_a_last_successful_read_visible_after_a_failure() {
    let previous = CodexOfficialQuota {
        status: CodexOfficialQuotaStatus::Available,
        windows: vec![CodexOfficialQuotaWindow {
            label: "5 小时".to_string(),
            used_percent: 38.0,
            resets_at: None,
        }],
        at: Some("2026-09-01T00:00:00Z".to_string()),
        stale: false,
        last_reset: None,
    };
    let retained = retain_last_success(
        empty(CodexOfficialQuotaStatus::Unavailable),
        Some(&previous),
    );
    assert_eq!(retained.status, CodexOfficialQuotaStatus::Unavailable);
    assert!(retained.stale);
    assert_eq!(retained.windows, previous.windows);
    assert_eq!(retained.at, previous.at);
}

#[test]
fn stale_quota_is_reused_only_for_the_same_logged_in_account() {
    let quota = weekly_quota("2026-09-01T00:00:00Z", 38.0, None);
    let entries = HashMap::from([(
        "official-profile".to_string(),
        CachedQuota {
            account_marker: "account-a".to_string(),
            quota: quota.clone(),
        },
    )]);

    assert_eq!(
        matching_last_success(&entries, "official-profile", Some("account-a")),
        Some(quota)
    );
    assert_eq!(
        matching_last_success(&entries, "official-profile", Some("account-b")),
        None
    );
    assert_eq!(
        matching_last_success(&entries, "official-profile", None),
        None
    );
}

#[test]
fn names_known_and_server_extension_windows() {
    assert_eq!(quota_window_label(2_592_000), "30 天");
    assert_eq!(quota_window_label(86_400), "1 天");
    assert_eq!(quota_window_label(7_200), "2 小时");
}

fn weekly_quota(at: &str, used_percent: f64, resets_at: Option<&str>) -> CodexOfficialQuota {
    CodexOfficialQuota {
        status: CodexOfficialQuotaStatus::Available,
        windows: vec![CodexOfficialQuotaWindow {
            label: WEEKLY_WINDOW_LABEL.to_string(),
            used_percent,
            resets_at: resets_at.map(str::to_string),
        }],
        at: Some(at.to_string()),
        stale: false,
        last_reset: None,
    }
}

fn baseline_from(quota: &CodexOfficialQuota, marker: Option<&str>) -> CodexQuotaBaseline {
    let (baseline, _) = apply_read(
        None,
        marker.map(str::to_string),
        quota,
        "2026-09-01T00:00:00Z",
    );
    baseline
}

#[test]
fn first_read_establishes_a_baseline_without_detection() {
    let quota = weekly_quota("2026-09-01T08:00:00Z", 40.0, Some("2026-09-04T00:00:00Z"));

    let (baseline, detected) = apply_read(
        None,
        Some("marker".to_string()),
        &quota,
        "2026-09-01T08:00:00Z",
    );

    assert_eq!(detected, None);
    assert_eq!(baseline.account_marker.as_deref(), Some("marker"));
    let read = baseline.last_read.expect("baseline read");
    assert_eq!(read.at, "2026-09-01T08:00:00Z");
    assert_eq!(read.windows, quota.windows);
    assert_eq!(baseline.last_reset, None);
}

#[test]
fn detects_a_scheduled_reset_when_usage_drops_after_the_declared_time() {
    let previous_quota = weekly_quota("2026-09-01T08:00:00Z", 80.0, Some("2026-09-02T00:00:00Z"));
    let baseline = baseline_from(&previous_quota, Some("marker"));
    let quota = weekly_quota("2026-09-02T06:00:00Z", 4.0, Some("2026-09-09T00:00:00Z"));

    let (baseline, detected) = apply_read(
        Some(baseline),
        Some("marker".to_string()),
        &quota,
        "2026-09-02T06:00:00Z",
    );

    let reset = detected.expect("scheduled reset");
    assert_eq!(reset.kind, CodexOfficialQuotaResetKind::Scheduled);
    assert_eq!(reset.observed_at, "2026-09-02T06:00:00Z");
    assert_eq!(reset.resets_at.as_deref(), Some("2026-09-09T00:00:00Z"));
    assert_eq!(baseline.last_reset, Some(reset));
}

#[test]
fn detects_an_early_reset_when_usage_drops_before_the_declared_time() {
    let previous_quota = weekly_quota("2026-09-01T08:00:00Z", 70.0, Some("2026-09-05T00:00:00Z"));
    let baseline = baseline_from(&previous_quota, Some("marker"));
    let quota = weekly_quota("2026-09-02T06:00:00Z", 3.0, Some("2026-09-09T00:00:00Z"));

    let (_, detected) = apply_read(
        Some(baseline),
        Some("marker".to_string()),
        &quota,
        "2026-09-02T06:00:00Z",
    );

    let reset = detected.expect("early reset");
    assert_eq!(reset.kind, CodexOfficialQuotaResetKind::Early);
}

#[test]
fn detects_a_moved_schedule_even_with_equal_usage() {
    let previous_quota = weekly_quota("2026-09-01T08:00:00Z", 50.0, Some("2026-09-04T00:00:00Z"));
    let baseline = baseline_from(&previous_quota, Some("marker"));
    let quota = weekly_quota("2026-09-02T06:00:00Z", 50.0, Some("2026-09-06T00:00:00Z"));

    let (_, detected) = apply_read(
        Some(baseline),
        Some("marker".to_string()),
        &quota,
        "2026-09-02T06:00:00Z",
    );

    let reset = detected.expect("moved schedule");
    assert_eq!(reset.kind, CodexOfficialQuotaResetKind::Early);
}

#[test]
fn keeps_a_known_reset_when_a_later_read_detects_nothing() {
    let previous_quota = weekly_quota("2026-09-01T08:00:00Z", 80.0, Some("2026-09-02T00:00:00Z"));
    let with_reset = baseline_from(&previous_quota, Some("marker"));
    let (with_reset, reset) = apply_read(
        Some(with_reset),
        Some("marker".to_string()),
        &weekly_quota("2026-09-02T06:00:00Z", 4.0, Some("2026-09-09T00:00:00Z")),
        "2026-09-02T06:00:00Z",
    );
    assert!(reset.is_some());
    let (baseline, detected) = apply_read(
        Some(with_reset.clone()),
        Some("marker".to_string()),
        &weekly_quota("2026-09-02T07:00:00Z", 5.0, Some("2026-09-09T00:00:00Z")),
        "2026-09-02T07:00:00Z",
    );

    assert_eq!(detected, None);
    assert_eq!(baseline.last_reset, with_reset.last_reset);
}

#[test]
fn a_changed_account_marker_wipes_the_history_instead_of_detecting() {
    let previous_quota = weekly_quota("2026-09-01T08:00:00Z", 80.0, Some("2026-09-02T00:00:00Z"));
    let mut baseline = baseline_from(&previous_quota, Some("old"));
    baseline.last_reset = Some(CodexOfficialQuotaReset {
        observed_at: "2026-09-01T09:00:00Z".to_string(),
        kind: CodexOfficialQuotaResetKind::Scheduled,
        resets_at: None,
    });
    let quota = weekly_quota("2026-09-02T06:00:00Z", 4.0, Some("2026-09-09T00:00:00Z"));

    let (baseline, detected) = apply_read(
        Some(baseline),
        Some("new".to_string()),
        &quota,
        "2026-09-02T06:00:00Z",
    );

    assert_eq!(detected, None);
    assert_eq!(baseline.account_marker.as_deref(), Some("new"));
    assert_eq!(baseline.last_reset, None);
    assert!(baseline.last_read.is_some());
}

#[test]
fn movement_of_other_windows_alone_is_ignored() {
    let previous_quota = CodexOfficialQuota {
        status: CodexOfficialQuotaStatus::Available,
        windows: vec![
            CodexOfficialQuotaWindow {
                label: "5 小时".to_string(),
                used_percent: 90.0,
                resets_at: Some("2026-09-01T10:00:00Z".to_string()),
            },
            CodexOfficialQuotaWindow {
                label: WEEKLY_WINDOW_LABEL.to_string(),
                used_percent: 40.0,
                resets_at: Some("2026-09-04T00:00:00Z".to_string()),
            },
        ],
        at: Some("2026-09-01T08:00:00Z".to_string()),
        stale: false,
        last_reset: None,
    };
    let baseline = baseline_from(&previous_quota, Some("marker"));
    let quota = CodexOfficialQuota {
        status: CodexOfficialQuotaStatus::Available,
        windows: vec![
            CodexOfficialQuotaWindow {
                label: "5 小时".to_string(),
                used_percent: 10.0,
                resets_at: Some("2026-09-01T15:00:00Z".to_string()),
            },
            CodexOfficialQuotaWindow {
                label: WEEKLY_WINDOW_LABEL.to_string(),
                used_percent: 42.0,
                resets_at: Some("2026-09-04T00:00:00Z".to_string()),
            },
        ],
        at: Some("2026-09-02T06:00:00Z".to_string()),
        stale: false,
        last_reset: None,
    };

    let (_, detected) = apply_read(
        Some(baseline),
        Some("marker".to_string()),
        &quota,
        "2026-09-02T06:00:00Z",
    );

    assert_eq!(detected, None);
}

#[test]
fn a_missing_weekly_window_skips_detection_but_still_updates_the_baseline() {
    let previous_quota = weekly_quota("2026-09-01T08:00:00Z", 40.0, Some("2026-09-04T00:00:00Z"));
    let baseline = baseline_from(&previous_quota, Some("marker"));
    let quota = CodexOfficialQuota {
        status: CodexOfficialQuotaStatus::Available,
        windows: vec![CodexOfficialQuotaWindow {
            label: "5 小时".to_string(),
            used_percent: 12.0,
            resets_at: None,
        }],
        at: Some("2026-09-02T06:00:00Z".to_string()),
        stale: false,
        last_reset: None,
    };

    let (baseline, detected) = apply_read(
        Some(baseline),
        Some("marker".to_string()),
        &quota,
        "2026-09-02T06:00:00Z",
    );

    assert_eq!(detected, None);
    assert!(baseline.last_read.is_some());
}

#[test]
fn a_previous_window_without_a_declared_time_counts_as_scheduled() {
    let previous_quota = weekly_quota("2026-09-01T08:00:00Z", 90.0, None);
    let baseline = baseline_from(&previous_quota, Some("marker"));
    let quota = weekly_quota("2026-09-02T06:00:00Z", 2.0, Some("2026-09-09T00:00:00Z"));

    let (_, detected) = apply_read(
        Some(baseline),
        Some("marker".to_string()),
        &quota,
        "2026-09-02T06:00:00Z",
    );

    assert_eq!(
        detected.expect("reset").kind,
        CodexOfficialQuotaResetKind::Scheduled
    );
}

#[test]
fn an_absent_marker_still_allows_detection() {
    let previous_quota = weekly_quota("2026-09-01T08:00:00Z", 80.0, Some("2026-09-02T00:00:00Z"));
    let baseline = baseline_from(&previous_quota, None);
    let quota = weekly_quota("2026-09-02T06:00:00Z", 4.0, Some("2026-09-09T00:00:00Z"));

    let (_, detected) = apply_read(Some(baseline), None, &quota, "2026-09-02T06:00:00Z");

    assert!(detected.is_some());
}

#[test]
fn the_account_marker_is_a_short_stable_digest() {
    let first = account_marker(Some("account-1")).expect("marker");
    let second = account_marker(Some("account-2")).expect("marker");
    assert_eq!(first.len(), 16);
    assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_ne!(first, second);
    assert_eq!(account_marker(Some("account-1")), Some(first));
    assert_eq!(account_marker(None), None);
}

#[test]
fn complete_oauth_residue_never_overrides_an_api_key_identity() {
    let valid = serde_json::json!({"auth_mode": "chatgpt", "tokens": {
        "access_token": "fixture-access", "refresh_token": "fixture-refresh", "id_token": "fixture-id", "account_id": "fixture-account"
    }});
    assert!(parse_credentials(&valid.to_string()).is_some());
    for key in ["access_token", "refresh_token", "id_token"] {
        let mut incomplete = valid.clone();
        incomplete["tokens"].as_object_mut().unwrap().remove(key);
        assert!(parse_credentials(&incomplete.to_string()).is_none());
    }
    let mut api_key = valid.clone();
    api_key["auth_mode"] = serde_json::json!("apikey");
    assert!(parse_credentials(&api_key.to_string()).is_none());
    let mut mixed = valid.clone();
    mixed["OPENAI_API_KEY"] = serde_json::json!("fixture-api-key");
    assert!(parse_credentials(&mixed.to_string()).is_none());
    let mut header = valid;
    header["tokens"]["access_token"] = serde_json::json!("token\r\nInjected: true");
    assert!(parse_credentials(&header.to_string()).is_none());
}

#[test]
fn unsupported_storage_never_uses_a_residual_file_for_quota() {
    let directory = tempfile::tempdir().unwrap();
    let auth = directory.path().join("auth.json");
    let content = r#"{"auth_mode":"chatgpt","tokens":{"access_token":"fixture-access","refresh_token":"fixture-refresh","id_token":"fixture-id","account_id":"fixture-account"}}"#;
    std::fs::write(&auth, content).unwrap();
    for mode in ["keyring", "auto", "ephemeral"] {
        std::fs::write(
            directory.path().join("config.toml"),
            format!("cli_auth_credentials_store = \"{mode}\"\n"),
        )
        .unwrap();
        let (quota, marker) = fetch(&auth);
        assert_eq!(quota.status, CodexOfficialQuotaStatus::SignInRequired);
        assert!(marker.is_none());
        assert_eq!(std::fs::read_to_string(&auth).unwrap(), content);
    }
}

#[test]
fn malformed_account_header_is_rejected_instead_of_querying_another_account() {
    let mut auth = serde_json::json!({"auth_mode": "chatgpt", "tokens": {
        "access_token": "fixture-access", "refresh_token": "fixture-refresh", "id_token": "fixture-id", "account_id": "fixture-account"
    }});
    for account in ["", " ", "fixture\r\nInjected: true", "fixture\taccount"] {
        auth["tokens"]["account_id"] = serde_json::json!(account);
        assert!(parse_credentials(&auth.to_string()).is_none());
    }
}
