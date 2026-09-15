use super::*;
use asb_core::contracts::{decimal_micros, UpstreamProtocol};
use serde_json::json;
use std::collections::BTreeMap;
const PROVIDER: &str = "00000000-0000-0000-0000-000000000001";
fn prices() -> BTreeMap<String, CodexModelPrice> {
    BTreeMap::from([(
        "actual-model".into(),
        CodexModelPrice {
            input_usd_per_million: "1".into(),
            output_usd_per_million: "2".into(),
            cache_read_usd_per_million: "0.1".into(),
            cache_creation_usd_per_million: "1.25".into(),
            source: "本机测试参考价".into(),
        },
    )])
}
fn record(at_ms: u64) -> CodexRequestRecord {
    serde_json::from_value(json!({
        "id": uuid::Uuid::new_v4().to_string(), "origin": "proxy", "atMs": at_ms, "billable": true,
        "profileId": PROVIDER, "routeRevision": "revision-a", "upstreamProtocol": "responses",
        "requestModel": "client-alias", "mappedModel": "actual-model", "responseModel": "response-model",
        "status": 200, "durationMs": 12, "firstByteLatencyMs": 4, "firstTokenLatencyMs": 5,
        "inputTokens": 1000, "outputTokens": 50, "cacheReadTokens": 200,
        "cacheCreationTokens": 100, "reasoningTokens": 40,
        "billing": { "costMultiplier": "2", "modelSource": "request",
            "dailyLimitUsd": null, "monthlyLimitUsd": null },
        "cost": null, "pricingError": null, "attempts": []
    })).unwrap()
}

/// A gateway-recorded turn with explicit usage, shared with session sync tests.
pub(super) fn proxy_record(
    timestamp: &str,
    model: &str,
    input: Option<u64>,
    output: Option<u64>,
    cache_read: Option<u64>,
) -> CodexRequestRecord {
    let at_ms = chrono::DateTime::parse_from_rfc3339(timestamp)
        .unwrap()
        .timestamp_millis()
        .max(0) as u64;
    let mut record = record(at_ms);
    record.mapped_model = Some(model.into());
    record.request_model = None;
    record.response_model = None;
    record.input_tokens = input;
    record.output_tokens = output;
    record.cache_read_tokens = cache_read;
    record.cache_creation_tokens = None;
    record.reasoning_tokens = None;
    record
}

/// A minimal valid session-sourced record, shared with session sync tests.
pub(super) fn session_record_shape(thread: &str, index: u32) -> CodexRequestRecord {
    CodexRequestRecord {
        id: session_request_id(thread, index),
        origin: CodexUsageOrigin::Session,
        thread_id: Some(thread.to_string()),
        at_ms: 1,
        billable: true,
        profile_id: None,
        route_revision: None,
        upstream_protocol: None,
        request_model: None,
        mapped_model: Some("gpt-5.4".into()),
        response_model: None,
        status: Some(200),
        duration_ms: 0,
        first_byte_latency_ms: None,
        first_token_latency_ms: None,
        input_tokens: Some(1),
        output_tokens: Some(1),
        cache_read_tokens: Some(0),
        cache_creation_tokens: None,
        reasoning_tokens: None,
        billing: Default::default(),
        cost: None,
        pricing_error: None,
        attempts: Vec::new(),
    }
}
#[test]
fn prices_the_actual_model_without_double_counting_caches_or_reasoning() {
    let mut entry = record(1);
    assert_eq!(
        estimate(&entry, &prices()).unwrap().unwrap().total_usd,
        "0.001890"
    );
    entry.billing.model_source = CodexPricingModelSource::Response;
    assert!(estimate(&entry, &prices()).unwrap().is_none());
    entry.response_model = None;
    assert_eq!(
        estimate(&entry, &prices()).unwrap().unwrap().model,
        "actual-model"
    );
    entry.input_tokens = None;
    assert!(estimate(&entry, &prices()).unwrap().is_none());
    entry.input_tokens = Some(100);
    assert!(estimate(&entry, &prices()).is_err());
}
#[test]
fn unknown_prices_are_not_zero_and_decimal_arithmetic_rejects_invalid_inputs() {
    let mut entry = record(1);
    assert!(estimate(&entry, &BTreeMap::new()).unwrap().is_none());
    for value in ["NaN", "-1", "1e2", "1.0000001"] {
        entry.billing.cost_multiplier = value.into();
        assert!(estimate(&entry, &prices()).is_err());
    }
    entry.billing.cost_multiplier = "0".into();
    assert_eq!(
        estimate(&entry, &prices()).unwrap().unwrap().total_usd,
        "0.000000"
    );
}
#[test]
fn restart_deduplication_and_conflicting_ids_keep_the_original_record() {
    let temporary = tempfile::tempdir().unwrap();
    let ledger = CodexRequestLedger::new(temporary.path());
    let original = record(1);
    ledger.append(&original).unwrap();
    ledger.append(&original).unwrap();
    let mut conflict = original.clone();
    conflict.duration_ms += 1;
    assert!(ledger.append(&conflict).unwrap_err().contains("内容不同"));
    let page = CodexRequestLedger::new(temporary.path())
        .page(&Default::default(), 0, 20)
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.records, [original]);
    assert!(!temporary.path().join("claude-request-ledger.json").exists());
}
#[test]
fn pagination_filters_and_utc_trends_distinguish_unknown_prices() {
    let temporary = tempfile::tempdir().unwrap();
    let ledger = CodexRequestLedger::new(temporary.path());
    let mut a = record(86_400_001);
    a.cost = estimate(&a, &prices()).unwrap();
    ledger.append(&a).unwrap();
    let mut b = record(172_800_001);
    b.status = Some(502);
    ledger.append(&b).unwrap();
    let mut c = record(172_800_002);
    c.billable = false;
    ledger.append(&c).unwrap();
    let all = CodexLedgerFilter::default();
    assert_eq!(ledger.page(&all, 0, 1).unwrap().records[0].id, c.id);
    assert_eq!(ledger.page(&all, 1, 1).unwrap().records[0].id, b.id);
    assert!(ledger.page(&all, 0, 0).is_err());
    let filter = CodexLedgerFilter {
        from_ms: Some(86_400_001),
        until_ms: Some(172_800_001),
        ..Default::default()
    };
    assert_eq!(ledger.page(&filter, 0, 20).unwrap().total, 1);
    let summary = ledger.summary(&all).unwrap();
    assert_eq!(
        (
            summary.requests,
            summary.priced_requests,
            summary.unpriced_requests
        ),
        (3, 1, 1)
    );
    assert_eq!(summary.failed_requests, 1);
    assert_eq!(summary.estimated_usd, "0.001890");
    assert_eq!(summary.days.len(), 2);
    assert_eq!(summary.days[0].start_ms, 86_400_000);
}
#[test]
fn settings_are_versioned_validated_and_compare_and_swapped() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let initial = read_settings(root).unwrap();
    assert!(!root.join("codex").exists());
    let mut settings = initial.settings;
    settings.prices = prices();
    settings
        .providers
        .insert(PROVIDER.into(), CodexBilling::default());
    let saved = save_settings(root, settings.clone(), &initial.revision).unwrap();
    assert!(save_settings(root, settings.clone(), &initial.revision).is_err());
    settings
        .providers
        .get_mut(PROVIDER)
        .unwrap()
        .daily_limit_usd = Some("0".into());
    assert!(save_settings(root, settings, &saved.revision).is_err());
    assert_eq!(read_settings(root).unwrap().revision, saved.revision);
}
#[test]
fn a_fresh_database_is_v2_and_unknown_historical_databases_are_preserved() {
    let temporary = tempfile::tempdir().unwrap();
    let ledger = CodexRequestLedger::new(temporary.path());
    ledger.append(&record(1)).unwrap();
    let db = rusqlite::Connection::open(&ledger.path).unwrap();
    let version: i64 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 2);
    db.pragma_update(None, "user_version", 7).unwrap();
    drop(db);
    let before = std::fs::read(&ledger.path).unwrap();
    assert!(ledger.append(&record(2)).is_err());
    assert!(ledger.summary(&Default::default()).is_err());
    assert_eq!(std::fs::read(&ledger.path).unwrap(), before);
    let other = tempfile::tempdir().unwrap();
    let ledger = CodexRequestLedger::new(other.path());
    std::fs::create_dir_all(ledger.path.parent().unwrap()).unwrap();
    let db = rusqlite::Connection::open(&ledger.path).unwrap();
    db.execute_batch("CREATE TABLE old_data(value TEXT); INSERT INTO old_data VALUES('keep')")
        .unwrap();
    drop(db);
    assert!(ledger
        .append(&record(1))
        .unwrap_err()
        .contains("schema 未知"));
    let db = rusqlite::Connection::open(&ledger.path).unwrap();
    assert_eq!(
        db.query_row("SELECT value FROM old_data", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
}
#[test]
fn concurrent_appends_never_lose_or_double_count_requests() {
    let temporary = tempfile::tempdir().unwrap();
    let ledger = CodexRequestLedger::new(temporary.path());
    let threads: Vec<_> = (0..12)
        .map(|n| {
            let ledger = ledger.clone();
            std::thread::spawn(move || ledger.append(&record(n)).unwrap())
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(ledger.summary(&Default::default()).unwrap().requests, 12);
}
#[test]
fn price_backfill_keeps_snapshot_identity_and_original_multiplier() {
    let temporary = tempfile::tempdir().unwrap();
    let ledger = CodexRequestLedger::new(temporary.path());
    let entry = record(1);
    ledger.append(&entry).unwrap();
    assert_eq!(ledger.reprice(&Default::default(), &prices()).unwrap(), 1);
    let updated = ledger
        .page(&Default::default(), 0, 10)
        .unwrap()
        .records
        .remove(0);
    assert_eq!(updated.id, entry.id);
    assert_eq!(updated.route_revision, entry.route_revision);
    assert_eq!(updated.billing.cost_multiplier, "2");
    assert_eq!(updated.cost.unwrap().total_usd, "0.001890");
    let mut broken = record(2);
    broken.input_tokens = Some(1);
    ledger.append(&broken).unwrap();
    assert!(ledger.reprice(&Default::default(), &prices()).is_err());
    assert!(ledger.page(&Default::default(), 0, 1).unwrap().records[0]
        .cost
        .is_none());
}
#[test]
fn budgets_use_utc_periods_and_unknown_billable_usage_is_not_silently_free() {
    let temporary = tempfile::tempdir().unwrap();
    let ledger = CodexRequestLedger::new(temporary.path());
    let now = 1_789_257_600_000; // 2026-09-13 UTC
    let mut entry = record(now);
    entry.cost = estimate(&entry, &prices()).unwrap();
    ledger.append(&entry).unwrap();
    let billing = CodexBilling {
        daily_limit_usd: Some("0.001".into()),
        ..Default::default()
    };
    assert_eq!(
        ledger.check_budget(PROVIDER, &billing, now).unwrap_err().0,
        429
    );
    assert!(ledger
        .check_budget(PROVIDER, &billing, now + 86_400_000)
        .is_ok());
    let unknown = record(now + 86_400_000);
    ledger.append(&unknown).unwrap();
    assert_eq!(
        ledger
            .check_budget(PROVIDER, &billing, now + 86_400_001)
            .unwrap_err()
            .0,
        503
    );
    assert!(ledger
        .check_budget(PROVIDER, &CodexBilling::default(), now + 86_400_001)
        .is_ok());
    assert!(ledger.check_budget("other", &billing, now).is_ok());
}
#[test]
fn corrupt_storage_and_overflowing_counters_remain_diagnosable() {
    let temporary = tempfile::tempdir().unwrap();
    let ledger = CodexRequestLedger::new(temporary.path());
    let mut entry = record(1);
    entry.input_tokens = Some(u64::MAX);
    assert!(ledger.append(&entry).is_err());
    std::fs::create_dir_all(ledger.path.parent().unwrap()).unwrap();
    std::fs::write(&ledger.path, "not sqlite").unwrap();
    assert!(ledger.page(&Default::default(), 0, 20).is_err());
    assert_eq!(std::fs::read_to_string(&ledger.path).unwrap(), "not sqlite");
    assert_eq!(decimal_micros("0.001890").unwrap(), 1890);
    assert_eq!(
        record(1).upstream_protocol,
        Some(UpstreamProtocol::Responses)
    );
}
