use super::*;
use crate::gateway::usage_metadata::{model_from_value, TokenUsage};
use asb_core::contracts::UpstreamProtocol;

fn ledger(directory: &tempfile::TempDir) -> ClaudeRequestLedger {
    ClaudeRequestLedger::new(directory.path().join("state/claude-request-ledger.json"))
}

fn record(at: &str, status: Option<u16>) -> ClaudeRequestRecord {
    ClaudeRequestRecord {
        at: at.to_string(),
        profile_id: Some("profile-1".to_string()),
        route_revision: Some("revision-1".to_string()),
        client_protocol: UpstreamProtocol::AnthropicMessages,
        upstream_protocol: Some(UpstreamProtocol::ChatCompletions),
        request_model: Some("claude-sonnet".to_string()),
        mapped_model: Some("vendor-sonnet".to_string()),
        response_model: None,
        cost: None,
        pricing_error: None,
        first_token_latency_ms: None,
        input_tokens: Some(10),
        output_tokens: Some(4),
        cache_read_tokens: Some(2),
        cache_creation_tokens: Some(1),
        reasoning_tokens: Some(3),
        status,
        duration_ms: 20,
        first_byte_latency_ms: Some(8),
        failover_attempts: Vec::new(),
    }
}

#[test]
fn persists_and_reads_after_a_new_handle_is_created() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state/claude-request-ledger.json");
    ledger(&directory)
        .append(record("2026-09-12T00:00:00.000Z", Some(200)))
        .unwrap();
    let page = ClaudeRequestLedger::new(path).page(0, 10, None).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(
        page.entries[0].request_model.as_deref(),
        Some("claude-sonnet")
    );
}

#[test]
fn pages_newest_first_and_reports_summary() {
    let directory = tempfile::tempdir().unwrap();
    let store = ledger(&directory);
    store
        .append(record("2026-09-12T00:00:00.000Z", Some(200)))
        .unwrap();
    store
        .append(record("2026-09-12T00:01:00.000Z", Some(502)))
        .unwrap();
    let page = store.page(0, 1, None).unwrap();
    assert_eq!(page.entries[0].at, "2026-09-12T00:01:00.000Z");
    assert!(page.has_more);
    let summary = store.summary(None).unwrap();
    assert_eq!(summary.total_requests, 2);
    assert_eq!(summary.failed_requests, 1);
    assert_eq!(summary.input_tokens, Some(20));
    assert_eq!(summary.cache_read_tokens, Some(4));
    assert_eq!(summary.average_first_byte_latency_ms, Some(8));
}

#[test]
fn rejects_unknown_fields_and_bad_versions() {
    let directory = tempfile::tempdir().unwrap();
    let store = ledger(&directory);
    let path = directory.path().join("state/claude-request-ledger.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, r#"{"version":1,"entries":[],"extra":true}"#).unwrap();
    assert!(store.page(0, 10, None).unwrap_err().contains("格式无效"));
    fs::write(&path, r#"{"version":99,"entries":[]}"#).unwrap();
    assert!(store.summary(None).unwrap_err().contains("版本"));
}

#[test]
fn serialized_ledger_has_no_request_secret_material() {
    let directory = tempfile::tempdir().unwrap();
    let store = ledger(&directory);
    let mut value = record("2026-09-12T00:00:00.000Z", Some(200));
    value.profile_id = Some("provider-profile".to_string());
    value.request_model = Some("requested-model".to_string());
    store.append(value).unwrap();
    let text =
        fs::read_to_string(directory.path().join("state/claude-request-ledger.json")).unwrap();
    for secret in [
        "api-key",
        "Authorization",
        "https://",
        "request body",
        "conversation",
    ] {
        assert!(!text.contains(secret), "ledger contained {secret}");
    }
}

#[test]
fn maps_protocol_usage_and_model_without_retaining_the_body() {
    let value = serde_json::json!({
        "model": "vendor-model",
        "usage": {
            "input_tokens": 12,
            "output_tokens": 5,
            "input_tokens_details": {"cached_tokens": 4},
            "output_tokens_details": {"reasoning_tokens": 2}
        }
    });
    assert_eq!(
        model_from_value(UpstreamProtocol::Responses, &value).as_deref(),
        Some("vendor-model")
    );
    assert_eq!(
        TokenUsage::from_value(UpstreamProtocol::Responses, value.get("usage")),
        TokenUsage {
            input_tokens: Some(12),
            output_tokens: Some(5),
            cache_read_tokens: Some(4),
            cache_creation_tokens: None,
            reasoning_tokens: Some(2),
        }
    );
}

#[test]
fn references_the_current_timestamp_shape() {
    DateTime::<FixedOffset>::parse_from_rfc3339(&now()).unwrap();
}

#[test]
fn filtered_pages_and_summaries_share_model_provider_time_and_error_scope() {
    let dir = tempfile::tempdir().unwrap();
    let store = ledger(&dir);
    let mut first = record("2026-09-13T10:00:00+08:00", Some(200));
    first.response_model = Some("served".into());
    first.cost = Some(ClaudeRequestCost {
        model: "served".into(),
        source: "local".into(),
        multiplier: "1".into(),
        total_usd: "0.100000".into(),
    });
    store.append(first).unwrap();
    store
        .append(record("2026-09-13T03:00:00Z", Some(502)))
        .unwrap();
    let filter = ClaudeLedgerFilter {
        model: Some("served".into()),
        from: Some("2026-09-13T01:00:00Z".into()),
        to: Some("2026-09-13T02:30:00Z".into()),
        ..Default::default()
    };
    assert_eq!(store.page(0, 10, Some(&filter)).unwrap().total, 1);
    let summary = store.summary(Some(&filter)).unwrap();
    assert_eq!(summary.total_requests, 1);
    assert_eq!(summary.estimated_cost_usd, "0.100000");
    assert_eq!(summary.priced_requests, 1);
    assert_eq!(summary.unpriced_requests, 0);
    let errors = ClaudeLedgerFilter {
        failures_only: true,
        ..Default::default()
    };
    assert_eq!(store.summary(Some(&errors)).unwrap().failed_requests, 1);
}

#[test]
fn records_written_before_cost_fields_remain_readable_without_guessing_prices() {
    let dir = tempfile::tempdir().unwrap();
    let store = ledger(&dir);
    let mut value = serde_json::to_value(record("2026-09-13T00:00:00Z", Some(200))).unwrap();
    for key in [
        "cost",
        "responseModel",
        "pricingError",
        "firstTokenLatencyMs",
    ] {
        value.as_object_mut().unwrap().remove(key);
    }
    fs::create_dir_all(store.path.parent().unwrap()).unwrap();
    fs::write(
        &store.path,
        serde_json::json!({"version":1,"entries":[value]}).to_string(),
    )
    .unwrap();
    let summary = store.summary(None).unwrap();
    assert_eq!(summary.priced_requests, 0);
    assert_eq!(summary.unpriced_requests, 1);
}

#[test]
fn v1_costs_migrate_once_and_pruning_detailed_records_does_not_reset_spend_limits() {
    let dir = tempfile::tempdir().unwrap();
    let store = ledger(&dir);
    let mut entry = record(&now(), Some(200));
    entry.cost = Some(ClaudeRequestCost {
        model: "priced".into(),
        source: "local".into(),
        multiplier: "1".into(),
        total_usd: "1.000000".into(),
    });
    fs::create_dir_all(store.path.parent().unwrap()).unwrap();
    fs::write(
        &store.path,
        serde_json::json!({"version":1,"entries":[entry.clone()]}).to_string(),
    )
    .unwrap();
    store.append(record(&now(), Some(200))).unwrap();
    let mut migrated = store.load().unwrap().unwrap();
    assert_eq!(migrated.version, 2);
    migrated.entries.clear();
    store.save(&migrated).unwrap();
    let billing = asb_core::contracts::ClaudeBilling {
        daily_limit_usd: Some("1".into()),
        ..Default::default()
    };
    assert!(store
        .budget_exceeded("profile-1", Some(&billing))
        .unwrap()
        .is_some());
    assert_eq!(
        super::spend::totals(
            &store.load().unwrap().unwrap().spend_by_day_utc,
            "profile-1",
            &Utc::now().date_naive().to_string()
        )
        .0,
        1_000_000
    );
}
