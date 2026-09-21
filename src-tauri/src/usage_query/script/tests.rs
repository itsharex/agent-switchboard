use super::*;

fn extract(output: &str) -> Result<UsageSummary, String> {
    let source = format!("({{ request() {{ return {{}}; }}, extract() {{ return {output}; }} }})");
    ScriptProgram::new(&source)?.extract(&serde_json::json!({}), 200, "2026-09-21T00:00:00Z".into())
}

#[test]
fn multiple_windows_preserve_reset_times_and_expired_plan_facts() {
    let summary = extract(r#"[
        { planName: "5h", remaining: 72, used: 28, total: 100, unit: "%", resetsAt: "2026-09-21T02:00:00Z" },
        { planName: "7d", remaining: null, used: null, total: null, unit: null,
          isValid: false, invalidMessage: "套餐已过期", extra: "需要续费" }
    ]"#).unwrap();
    assert_eq!(summary.readings.len(), 2);
    assert_eq!(summary.readings[0].resets_at.as_deref(), Some("2026-09-21T02:00:00Z"));
    assert_eq!(summary.readings[1].is_valid, Some(false));
    assert_eq!(summary.readings[1].invalid_message.as_deref(), Some("套餐已过期"));
    assert_eq!(summary.readings[1].extra.as_deref(), Some("需要续费"));
    assert!(summary.readings[1].remaining.is_none());
}

#[test]
fn malformed_metadata_and_unknown_fields_fail_instead_of_becoming_fake_balances() {
    assert!(extract(r#"{remaining: 1, resetsAt: "tomorrow"}"#).is_err());
    assert!(extract(r#"{remaining: 1, invalidMessage: "expired"}"#).is_err());
    assert!(extract(r#"{remaining: 1, resetAt: "2026-09-21T02:00:00Z"}"#).is_err());
    assert!(extract(r#"{remaining: null, used: null, total: null}"#).is_err());
}

#[test]
fn native_zhipu_windows_keep_reset_times_and_skip_missing_percentages() {
    let source = include_str!("../../../../crates/asb-core/src/ccswitch/templates/zhipu_token_plan_usage.js");
    let program = ScriptProgram::new(source).unwrap();
    let body = serde_json::json!({ "data": { "limits": [
        { "type": "TOKENS_LIMIT", "unit": 3, "percentage": 28, "nextResetTime": 1789956000000_i64 },
        { "type": "TOKENS_LIMIT", "unit": 6, "percentage": 54 },
        { "type": "TOKENS_LIMIT", "unit": 3 }
    ] } });
    let summary = program.extract(&body, 200, "2026-09-21T00:00:00Z".into()).unwrap();
    assert_eq!(summary.readings.len(), 2);
    assert_eq!(summary.readings[0].unit.as_deref(), Some("%"));
    assert_eq!(summary.readings[0].remaining, Some(72.0));
    assert!(summary.readings[0].resets_at.is_some());
    assert!(summary.readings[1].resets_at.is_none());
}

#[test]
fn imported_token_plan_metadata_is_normalized_before_native_validation() {
    let source = include_str!("../../../../crates/asb-core/src/ccswitch/templates/script_adapter.js")
        .replace("__ASB_TOKEN_PLAN__", "true")
        .replace("__ASB_CC_SOURCE__", r#"{
            request: { url: "https://example.invalid" },
            extractor: (body) => body
        }"#);
    let body = serde_json::json!({
        "planName": "five_hour", "used": 28,
        "extra": "{\"resetsAt\":\"2026-09-21T02:00:00Z\",\"planLabel\":\"Pro\"}"
    });
    let summary = ScriptProgram::new(&source).unwrap()
        .extract(&body, 200, "2026-09-21T00:00:00Z".into()).unwrap();
    let reading = &summary.readings[0];
    assert_eq!(reading.remaining, Some(72.0));
    assert_eq!(reading.unit.as_deref(), Some("%"));
    assert_eq!(reading.resets_at.as_deref(), Some("2026-09-21T02:00:00Z"));
    assert_eq!(reading.extra.as_deref(), Some("Pro"));
}
