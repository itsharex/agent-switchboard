use super::parse::local_date;
use super::*;

use std::fs;
use tempfile::tempdir;

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    fs::write(path, content).expect("write session");
}

fn timestamp(days_before_now: i64) -> String {
    (Local::now() - Duration::days(days_before_now)).to_rfc3339()
}

fn roots(temp: &tempfile::TempDir) -> Vec<(AppKind, std::path::PathBuf)> {
    vec![
        (AppKind::Codex, temp.path().join("codex")),
        (AppKind::Claude, temp.path().join("claude")),
    ]
}

#[test]
fn aggregates_codex_cumulative_counts_by_turn_context_model() {
    let temp = tempdir().expect("temp");
    let at = timestamp(0);
    write(
            &temp.path().join("codex").join("usage.jsonl"),
            &format!(
                concat!(
                    "{{\"type\":\"turn_context\",\"payload\":{{\"model\":\"gpt-5\"}}}}\n",
                    "{{\"timestamp\":\"{at}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"input_tokens\":10,\"cached_input_tokens\":2,\"output_tokens\":3,\"total_tokens\":13}}}}}}}}\n",
                    "{{\"timestamp\":\"{at}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"input_tokens\":16,\"cached_input_tokens\":4,\"output_tokens\":5,\"total_tokens\":21}}}}}}}}\n",
                    "{{\"type\":\"turn_context\",\"payload\":{{\"model\":\"gpt-4\"}}}}\n",
                    "{{\"timestamp\":\"{at}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"input_tokens\":3,\"cached_input_tokens\":0,\"output_tokens\":1,\"total_tokens\":4}}}}}}}}\n",
                    "{{\"timestamp\":\"{at}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"input_tokens\":5,\"cached_input_tokens\":1,\"output_tokens\":2,\"total_tokens\":7}}}}}}}}\n"
                ),
                at = at
            ),
        );

    let report = report_from_roots(ModelUsageRange::Today, &roots(&temp), Local::now());
    assert!(report.issues.is_empty());
    assert_eq!(report.groups.len(), 2);
    assert_eq!(
        report.groups[0],
        ModelUsageGroup {
            app: AppKind::Codex,
            model: Some("gpt-4".to_string()),
            input_tokens: 1,
            cache_read_input_tokens: 1,
            cache_creation_input_tokens: 0,
            output_tokens: 1,
            total_tokens: 3,
            session_count: 1,
        }
    );
    assert_eq!(
        report.groups[1],
        ModelUsageGroup {
            app: AppKind::Codex,
            model: Some("gpt-5".to_string()),
            input_tokens: 12,
            cache_read_input_tokens: 4,
            cache_creation_input_tokens: 0,
            output_tokens: 5,
            total_tokens: 21,
            session_count: 1,
        }
    );
    assert_eq!(
        report.days,
        vec![ModelUsageDay {
            date: local_date(&at)
                .expect("local date")
                .format("%F")
                .to_string(),
            input_tokens: 13,
            cache_read_input_tokens: 5,
            cache_creation_input_tokens: 0,
            output_tokens: 6,
            total_tokens: 24,
        }]
    );
    assert_eq!(report.unassigned_tokens, ModelUsageTokens::default());
}

#[test]
fn counts_only_explicit_claude_usage_and_keeps_partial_results() {
    let temp = tempdir().expect("temp");
    let at = timestamp(0);
    write(
            &temp.path().join("claude").join("usage.jsonl"),
            &format!(
                concat!(
                    "not json\n",
                    "{{\"timestamp\":\"{at}\",\"type\":\"assistant\",\"message\":{{\"id\":\"message-1\",\"model\":\"claude-sonnet\",\"usage\":{{\"input_tokens\":10,\"cache_creation_input_tokens\":3,\"cache_read_input_tokens\":2,\"output_tokens\":4}}}}}}\n",
                    "{{\"timestamp\":\"{at}\",\"type\":\"assistant\",\"message\":{{\"id\":\"message-zero\",\"model\":\"claude-sonnet\",\"usage\":{{\"input_tokens\":0,\"output_tokens\":0}}}}}}\n",
                    "{{\"timestamp\":\"{at}\",\"type\":\"assistant\",\"message\":{{\"usage\":{{\"input_tokens\":99,\"output_tokens\":99}}}}}}\n"
                ),
                at = at
            ),
        );

    let report = report_from_roots(ModelUsageRange::Today, &roots(&temp), Local::now());
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].app, AppKind::Claude);
    assert_eq!(
        report.groups,
        vec![ModelUsageGroup {
            app: AppKind::Claude,
            model: Some("claude-sonnet".to_string()),
            input_tokens: 10,
            cache_read_input_tokens: 2,
            cache_creation_input_tokens: 3,
            output_tokens: 4,
            total_tokens: 19,
            session_count: 1,
        }]
    );
}

#[test]
fn time_ranges_use_the_local_calendar_but_keep_the_cumulative_baseline() {
    let temp = tempdir().expect("temp");
    let old = timestamp(8);
    let current = timestamp(0);
    write(
            &temp.path().join("codex").join("usage.jsonl"),
            &format!(
                concat!(
                    "{{\"type\":\"turn_context\",\"payload\":{{\"model\":\"gpt-5\"}}}}\n",
                    "{{\"timestamp\":\"{old}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"input_tokens\":4,\"cached_input_tokens\":1,\"output_tokens\":1,\"total_tokens\":5}}}}}}}}\n",
                    "{{\"timestamp\":\"{current}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"input_tokens\":7,\"cached_input_tokens\":2,\"output_tokens\":3,\"total_tokens\":10}}}}}}}}\n"
                ),
                old = old,
                current = current
            ),
        );

    let report = report_from_roots(ModelUsageRange::Last7Days, &roots(&temp), Local::now());
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].input_tokens, 2);
    assert_eq!(report.groups[0].cache_read_input_tokens, 1);
    assert_eq!(report.groups[0].cache_creation_input_tokens, 0);
    assert_eq!(report.groups[0].output_tokens, 2);
    assert_eq!(report.groups[0].total_tokens, 5);
    assert_eq!(
        report.days,
        vec![ModelUsageDay {
            date: local_date(&current)
                .expect("current local date")
                .format("%F")
                .to_string(),
            input_tokens: 2,
            cache_read_input_tokens: 1,
            cache_creation_input_tokens: 0,
            output_tokens: 2,
            total_tokens: 5,
        }]
    );
}

#[test]
fn claude_duplicates_choose_a_completed_message_then_largest_output() {
    let temp = tempdir().expect("temp");
    let at = timestamp(0);
    write(
            &temp.path().join("claude").join("usage.jsonl"),
            &format!(
                concat!(
                    "{{\"timestamp\":\"{at}\",\"type\":\"assistant\",\"message\":{{\"id\":\"message-1\",\"model\":\"claude-sonnet\",\"usage\":{{\"input_tokens\":2,\"output_tokens\":9}}}}}}\n",
                    "{{\"timestamp\":\"{at}\",\"type\":\"assistant\",\"message\":{{\"id\":\"message-1\",\"model\":\"claude-sonnet\",\"stop_reason\":\"end_turn\",\"usage\":{{\"input_tokens\":3,\"cache_creation_input_tokens\":4,\"cache_read_input_tokens\":5,\"output_tokens\":6}}}}}}\n",
                    "{{\"timestamp\":\"{at}\",\"type\":\"assistant\",\"message\":{{\"id\":\"message-1\",\"model\":\"claude-sonnet\",\"stop_reason\":\"end_turn\",\"usage\":{{\"input_tokens\":7,\"cache_creation_input_tokens\":8,\"cache_read_input_tokens\":9,\"output_tokens\":10}}}}}}\n"
                ),
                at = at
            ),
        );

    let report = report_from_roots(ModelUsageRange::Today, &roots(&temp), Local::now());
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].input_tokens, 7);
    assert_eq!(report.groups[0].cache_read_input_tokens, 9);
    assert_eq!(report.groups[0].cache_creation_input_tokens, 8);
    assert_eq!(report.groups[0].output_tokens, 10);
    assert_eq!(report.groups[0].total_tokens, 34);
    assert_eq!(report.groups[0].session_count, 1);
}

#[test]
fn claude_duplicate_uses_the_selected_completion_timestamp_for_its_daily_bucket() {
    let temp = tempdir().expect("temp");
    let previous = timestamp(1);
    let current = timestamp(0);
    write(
            &temp.path().join("claude").join("usage.jsonl"),
            &format!(
                concat!(
                    "{{\"timestamp\":\"{previous}\",\"type\":\"assistant\",\"message\":{{\"id\":\"message-1\",\"model\":\"claude-sonnet\",\"usage\":{{\"input_tokens\":2,\"output_tokens\":3}}}}}}\n",
                    "{{\"timestamp\":\"{current}\",\"type\":\"assistant\",\"message\":{{\"id\":\"message-1\",\"model\":\"claude-sonnet\",\"stop_reason\":\"end_turn\",\"usage\":{{\"input_tokens\":4,\"output_tokens\":5}}}}}}\n"
                ),
                previous = previous,
                current = current,
            ),
        );

    let report = report_from_roots(ModelUsageRange::Last7Days, &roots(&temp), Local::now());

    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].total_tokens, 9);
    assert_eq!(
        report.days,
        vec![ModelUsageDay {
            date: local_date(&current)
                .expect("current local date")
                .format("%F")
                .to_string(),
            input_tokens: 4,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
            output_tokens: 5,
            total_tokens: 9,
        }]
    );
}

#[test]
fn duplicate_archived_session_keeps_the_first_configured_root() {
    let temp = tempdir().expect("temp");
    let at = timestamp(0);
    let record = |session_id: &str, input: u64| {
        [
            serde_json::json!({"type": "session_meta", "payload": {"id": session_id}}),
            serde_json::json!({"type": "turn_context", "payload": {"model": "gpt-5"}}),
            serde_json::json!({
                "timestamp": at,
                "type": "event_msg",
                "payload": {
                    "type": "token_count",
                    "info": {"total_token_usage": {
                        "input_tokens": input,
                        "cached_input_tokens": 0,
                        "output_tokens": 0,
                        "total_tokens": input
                    }}
                }
            }),
        ]
        .into_iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join("\n")
            + "\n"
    };
    let active = temp.path().join("active").join("usage.jsonl");
    let archived = temp.path().join("archived").join("usage.jsonl");
    write(&active, &record("session-1", 5));
    write(&archived, &record("session-1", 100));

    let report = report_from_roots(
        ModelUsageRange::Today,
        &[
            (AppKind::Codex, temp.path().join("active")),
            (AppKind::Codex, temp.path().join("archived")),
        ],
        Local::now(),
    );
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].input_tokens, 5);
    assert_eq!(report.groups[0].session_count, 1);
}

#[test]
fn all_range_reports_undated_tokens_separately_from_the_daily_trend() {
    let temp = tempdir().expect("temp");
    write(
            &temp.path().join("codex").join("usage.jsonl"),
            concat!(
                "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":4,\"cached_input_tokens\":1,\"output_tokens\":2,\"total_tokens\":6}}}}\n"
            ),
        );

    let all = report_from_roots(ModelUsageRange::All, &roots(&temp), Local::now());
    assert_eq!(all.groups.len(), 1);
    assert!(all.days.is_empty());
    assert_eq!(
        all.unassigned_tokens,
        ModelUsageTokens {
            input_tokens: 3,
            cache_read_input_tokens: 1,
            cache_creation_input_tokens: 0,
            output_tokens: 2,
            total_tokens: 6,
        }
    );

    let today = report_from_roots(ModelUsageRange::Today, &roots(&temp), Local::now());
    assert!(today.groups.is_empty());
    assert_eq!(today.unassigned_tokens, ModelUsageTokens::default());
}
