use super::*;

fn parse_fixture(events: &str, next_schedule: &str, monitor_status: &str) -> CodexResetStatus {
    let body = format!(
        r#"{{
                "schemaVersion": 1,
                "generatedAt": "2026-08-31T03:08:02.232Z",
                "lastSuccessfulCheckAt": "2026-08-31T03:08:02.232Z",
                "monitor": {{ "status": "{monitor_status}" }},
                "events": [{events}],
                "resetTimeline": {{ "nextSchedule": {next_schedule} }}
            }}"#
    );
    parse_feed(&body, "2026-08-31T03:10:00Z".to_string()).expect("feed")
}

#[test]
fn normalizes_confirmed_reset_schedule_and_latest_tibo_post() {
    let older = r#"{
            "kind":"reset_completed",
            "announcedAt":"2026-08-30T02:00:00Z",
            "confidence":0.9,
            "source":{"handle":"thsottiaux","url":"https://x.com/thsottiaux/status/100"},
            "text":"Older reset signal"
        }"#;
    let newest = r#"{
            "kind":"reset_completed",
            "announcedAt":"2026-08-31T02:34:27Z",
            "confidence":0.98,
            "source":{"handle":"thsottiaux","url":"https://x.com/thsottiaux/status/200"},
            "text":"Newer reset signal\nwith a line break"
        }"#;
    let schedule = r#"{
            "kind":"reset_scheduled",
            "announcedAt":"2026-08-31T03:00:00Z",
            "effectiveAt":"2026-08-31T09:00:00Z",
            "schedulePrecision":"datetime",
            "confidence":0.84,
            "source":{"handle":"thsottiaux","url":"https://x.com/thsottiaux/status/300"},
            "text":"Newly announced schedule"
        }"#;

    let status = parse_fixture(&format!("{older},{newest}"), schedule, "ok");

    assert_eq!(status.source_url, STATUS_URL);
    assert_eq!(status.feed_status, CodexResetFeedStatus::Ok);
    assert_eq!(
        status
            .latest_confirmed_signal
            .as_ref()
            .map(|reset| &reset.announced_at),
        Some(&"2026-08-31T02:34:27Z".to_string())
    );
    assert_eq!(
        status
            .next_scheduled_reset
            .as_ref()
            .and_then(|reset| reset.effective_at.as_deref()),
        Some("2026-08-31T09:00:00Z")
    );
    assert_eq!(
        status
            .latest_relevant_tibo_post
            .as_ref()
            .map(|post| post.text.as_str()),
        Some("Newly announced schedule")
    );
}

#[test]
fn preserves_an_absent_schedule_and_marks_degraded_source() {
    let status = parse_fixture("", "null", "error");

    assert_eq!(status.feed_status, CodexResetFeedStatus::Degraded);
    assert!(status.latest_confirmed_signal.is_none());
    assert!(status.next_scheduled_reset.is_none());
    assert!(status.latest_relevant_tibo_post.is_none());
    assert!(status.source_warning.is_some());
}

#[test]
fn ignores_untrusted_tibo_links() {
    let event = r#"{
            "kind":"reset_completed",
            "announcedAt":"2026-08-31T02:34:27Z",
            "confidence":0.98,
            "source":{"handle":"thsottiaux","url":"https://example.com/thsottiaux/status/200"},
            "text":"Do not expose this link"
        }"#;
    let status = parse_fixture(event, "null", "ok");

    assert!(status.latest_relevant_tibo_post.is_none());
}

#[test]
fn normalizes_a_manually_confirmed_reset_card_from_the_timeline() {
    let body = r#"{
            "schemaVersion": 1,
            "generatedAt": "2026-09-04T08:40:02.783Z",
            "lastSuccessfulCheckAt": "2026-09-04T08:40:02.783Z",
            "monitor": { "status": "ok" },
            "events": [],
            "resetTimeline": {
                "nextSchedule": null,
                "manualCompletions": [{
                    "completedAt": "2026-09-04T02:30:00Z",
                    "schedules": [{
                        "kind": "reset_scheduled",
                        "resetType": "banked",
                        "announcedAt": "2026-09-03T23:12:09Z",
                        "effectiveAt": "2026-09-04T02:12:09Z",
                        "schedulePrecision": "datetime",
                        "confidence": 0.93,
                        "source": {
                            "handle": "thsottiaux",
                            "url": "https://x.com/thsottiaux/status/2095651088502591861"
                        },
                        "text": "The first reset card has landed."
                    }]
                }]
            }
        }"#;

    let status = parse_feed(body, "2026-09-04T08:45:00Z".to_string()).expect("feed");
    let signal = status.latest_confirmed_signal.expect("completed signal");

    assert_eq!(signal.announced_at, "2026-09-04T02:30:00Z");
    assert_eq!(signal.reset_type, ResetType::Banked);
    assert_eq!(
        status.latest_relevant_tibo_post.map(|post| post.text),
        Some("The first reset card has landed.".to_string())
    );
}

#[test]
fn rejects_unknown_schemas_and_malformed_timestamps() {
    let unsupported = r#"{
            "schemaVersion": 2,
            "generatedAt":"2026-08-31T03:08:02.232Z",
            "lastSuccessfulCheckAt":"2026-08-31T03:08:02.232Z",
            "monitor":{"status":"ok"}
        }"#;
    assert!(parse_feed(unsupported, "2026-08-31T03:10:00Z".to_string()).is_err());

    let invalid_time = r#"{
            "schemaVersion": 1,
            "generatedAt":"not-a-time",
            "lastSuccessfulCheckAt":"2026-08-31T03:08:02.232Z",
            "monitor":{"status":"ok"}
        }"#;
    assert!(parse_feed(invalid_time, "2026-08-31T03:10:00Z".to_string()).is_err());
}
