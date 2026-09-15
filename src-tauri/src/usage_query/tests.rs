use super::declarative::{extract_declarative_summary, pointer_number, render_url};
use super::script::{
    parse_script_request, ScriptProgram, SCRIPT_EXECUTION_FAILED, SCRIPT_PROGRAM_INVALID,
};
use super::*;
use std::time::{Duration, Instant};

fn declarative(
    url: &str,
    remaining: Option<&str>,
    used: Option<&str>,
    total: Option<&str>,
) -> UsageQuery {
    UsageQuery::Declarative {
        url: url.to_string(),
        remaining_path: remaining.map(str::to_string),
        used_path: used.map(str::to_string),
        total_path: total.map(str::to_string),
        unit: None,
        refresh_interval_minutes: 0,
    }
}

const SCRIPT: &str = r#"({
        request({ baseUrl, apiKey }) {
            return {
                url: baseUrl + "/usage",
                method: "POST",
                headers: { Authorization: "Bearer " + apiKey },
                body: "{}"
            };
        },
        extract({ body, status }) {
            return { remaining: body.balance, used: status, unit: "credits" };
        }
    })"#;

#[test]
fn placeholders_substitute_and_base_url_loses_trailing_slash() {
    let rendered = render_url(
        "{{baseUrl}}/user/balance?key={{apiKey}}",
        "sk-x",
        Some("https://relay.example/v1/"),
    );
    assert_eq!(rendered, "https://relay.example/v1/user/balance?key=sk-x");
}

#[test]
fn pointer_reads_numbers_numeric_strings_and_array_cells() {
    let body: serde_json::Value =
        serde_json::from_str(r#"{"data":{"balance":12.5,"used":"3.25","tiers":[{"quota":7}]}}"#)
            .expect("body");
    assert_eq!(pointer_number(&body, "data/balance"), Some(12.5));
    assert_eq!(pointer_number(&body, "/data/used"), Some(3.25));
    assert_eq!(pointer_number(&body, "data/tiers/0/quota"), Some(7.0));
    assert_eq!(pointer_number(&body, "data/missing"), None);
    assert_eq!(pointer_number(&body, ""), None);
}

#[test]
fn declarative_summary_fills_configured_fields_only() {
    let body: serde_json::Value =
        serde_json::from_str(r#"{"balance":9.5,"used":0.5}"#).expect("body");
    let summary = extract_declarative_summary(
        &body,
        Some("balance"),
        None,
        Some("total"),
        Some("USD".into()),
        "t".into(),
    )
    .expect("summary");
    assert_eq!(summary.readings.len(), 1);
    let reading = &summary.readings[0];
    assert_eq!(reading.remaining, Some(9.5));
    assert_eq!(reading.used, None);
    assert_eq!(reading.total, None);
    assert_eq!(reading.unit.as_deref(), Some("USD"));
}

#[test]
fn declarative_validation_happens_before_any_request() {
    let empty_url = declarative("  ", Some("a"), None, None);
    assert!(run_usage_query(&empty_url, "sk", None, UpstreamProtocol::Responses, None).is_err());
    let no_paths = declarative("https://x", None, None, None);
    assert!(run_usage_query(&no_paths, "sk", None, UpstreamProtocol::Responses, None).is_err());
}

#[test]
fn script_keeps_request_and_extract_in_one_bounded_runtime() {
    let program = ScriptProgram::new(SCRIPT).expect("script program");
    let request = program
        .request("test-api-key", Some("https://relay.example/v1"))
        .expect("request");
    assert_eq!(request.method, "POST");
    assert_eq!(request.url, "https://relay.example/v1/usage");
    assert_eq!(
        request.headers.get("Authorization"),
        Some(&"Bearer test-api-key".to_string())
    );
    let response = serde_json::json!({ "balance": 12.5 });
    let summary = program
        .extract(&response, 201, "t".to_string())
        .expect("extract");
    assert_eq!(summary.readings.len(), 1);
    let reading = &summary.readings[0];
    assert_eq!(reading.remaining, Some(12.5));
    assert_eq!(reading.used, Some(201.0));
    assert_eq!(reading.unit.as_deref(), Some("credits"));
}

#[test]
fn script_extract_preserves_multiple_named_readings() {
    let program = ScriptProgram::new(
        r#"({
                request() { return { url: "https://relay.example", method: "GET" }; },
                extract() {
                    return [
                        { planName: "主套餐", remaining: 12, unit: "USD" },
                        { planName: "附加套餐", used: 3, total: 10, unit: "USD" }
                    ];
                }
            })"#,
    )
    .expect("script program");

    let summary = program
        .extract(&serde_json::json!({}), 200, "t".to_string())
        .expect("multiple readings");
    assert_eq!(summary.readings.len(), 2);
    assert_eq!(summary.readings[0].plan_name.as_deref(), Some("主套餐"));
    assert_eq!(summary.readings[0].remaining, Some(12.0));
    assert_eq!(summary.readings[1].plan_name.as_deref(), Some("附加套餐"));
    assert_eq!(summary.readings[1].used, Some(3.0));
    assert_eq!(summary.readings[1].total, Some(10.0));
}

#[test]
fn imported_ccswitch_script_uses_profile_inputs_and_keeps_each_plan() {
    let row = asb_core::ccswitch::CcSwitchRow {
            id: "cc-usage".to_string(),
            app_type: "claude".to_string(),
            name: "导入测试".to_string(),
            settings_config: r#"{
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay.example/v1",
                    "ANTHROPIC_AUTH_TOKEN": "<placeholder>"
                }
            }"#
            .to_string(),
            website_url: None,
            notes: None,
            display: None,
            meta: Some(
                serde_json::json!({
                    "usage_script": {
                        "enabled": true,
                        "language": "javascript",
                        "code": r#"({
                            request: {
                                url: "{{baseUrl}}/balance",
                                method: "GET",
                                headers: { Authorization: "Bearer {{apiKey}}" }
                            },
                            extractor: function(response) {
                                return [
                                    { isValid: true, planName: "主套餐", remaining: response.main, unit: "USD" },
                                    { isValid: true, planName: "附加套餐", total: response.total, used: response.used, unit: "USD" }
                                ];
                            }
                        })"#
                    }
                })
                .to_string(),
            ),
        };
    let proposal = asb_core::ccswitch::map_row(&row).expect("mapped provider");
    let asb_core::ccswitch::CcSwitchProviderDraft::Claude(draft) = proposal.draft else {
        panic!("Claude source must produce a Claude import draft");
    };
    let Some(UsageQuery::Script { source, .. }) = draft.usage_query else {
        panic!("query script should import");
    };

    let program = ScriptProgram::new(&source).expect("native script");
    let request = program
        .request("profile-key", Some("https://relay.example/v1/"))
        .expect("request");
    assert_eq!(request.url, "https://relay.example/v1/balance");
    assert_eq!(
        request.headers.get("Authorization"),
        Some(&"Bearer profile-key".to_string())
    );
    let summary = program
        .extract(
            &serde_json::json!({ "main": 12, "used": 3, "total": 20 }),
            200,
            "t".to_string(),
        )
        .expect("summary");
    assert_eq!(summary.readings.len(), 2);
    assert_eq!(summary.readings[0].plan_name.as_deref(), Some("主套餐"));
    assert_eq!(summary.readings[0].remaining, Some(12.0));
    assert_eq!(summary.readings[1].plan_name.as_deref(), Some("附加套餐"));
    assert_eq!(summary.readings[1].used, Some(3.0));
    assert_eq!(summary.readings[1].total, Some(20.0));
}

#[test]
fn script_validation_requires_both_functions_without_leaking_source() {
    let query = UsageQuery::Script {
        source: "({ request() {} })".to_string(),
        refresh_interval_minutes: 0,
    };
    let error = validate_persisted(&query).expect_err("missing extract");
    assert_eq!(error, SCRIPT_PROGRAM_INVALID);
    assert!(!error.contains("request()"));
}

#[test]
fn script_request_rejects_invalid_method_urls_and_header_lines() {
    for value in [
        serde_json::json!({ "url": "file:///x", "method": "GET" }),
        serde_json::json!({ "url": "https://x", "method": "PATCH" }),
        serde_json::json!({ "url": "https://x", "method": "GET", "headers": { "X-Test": "ok\r\nInjected: yes" } }),
    ] {
        assert!(parse_script_request(value).is_err());
    }
}

#[test]
fn script_extract_requires_one_numeric_value() {
    let program = ScriptProgram::new(
        r#"({
                request() { return { url: "https://relay.example", method: "GET" }; },
                extract() { return { unit: "credits" }; }
            })"#,
    )
    .expect("script program");
    assert!(program
        .extract(&serde_json::json!({}), 200, "t".to_string())
        .is_err());
}

#[test]
fn scripts_have_no_host_network_or_process_globals() {
    let program = ScriptProgram::new(
        r#"({
                request() {
                    if (
                        typeof fetch !== "undefined" ||
                        typeof process !== "undefined" ||
                        typeof module !== "undefined" ||
                        typeof require !== "undefined" ||
                        typeof fs !== "undefined" ||
                        typeof os !== "undefined" ||
                        typeof std !== "undefined" ||
                        typeof Deno !== "undefined" ||
                        typeof Bun !== "undefined"
                    ) throw new Error("host capability");
                    return { url: "https://relay.example", method: "GET" };
                },
                extract() { return { remaining: 1 }; }
            })"#,
    )
    .expect("script program");
    assert!(program.request("key", None).is_ok());
}

#[test]
fn script_errors_do_not_echo_api_keys() {
    let secret = "api-key-that-must-not-escape";
    let program = ScriptProgram::new(
        r#"({
                request({ apiKey }) { throw new Error(apiKey); },
                extract() { return { remaining: 1 }; }
            })"#,
    )
    .expect("script program");

    let error = program
        .request(secret, None)
        .expect_err("request must fail");
    assert_eq!(error, SCRIPT_EXECUTION_FAILED);
    assert!(!error.contains(secret));
}

#[test]
fn nonterminating_source_is_interrupted() {
    let started = Instant::now();
    assert!(ScriptProgram::new("(() => { while (true) {} })()").is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
}

/// The vendored template scripts synthesized by the import must run as real
/// native programs: the engine drives request and extract end to end.
#[test]
fn imported_template_scripts_run_in_the_script_engine() {
    let row = asb_core::ccswitch::CcSwitchRow {
        id: "tpl-1".to_string(),
        app_type: "claude".to_string(),
        name: "DeepSeek".to_string(),
        settings_config: r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.deepseek.com","ANTHROPIC_AUTH_TOKEN":"token","ANTHROPIC_MODEL":"deepseek-chat"}}"#.to_string(),
        website_url: None,
        notes: None,
        display: None,
        meta: Some(
            r#"{"usage_script":{"enabled":true,"language":"javascript","code":"","templateType":"balance"}}"#.to_string(),
        ),
    };
    let proposal = asb_core::ccswitch::map_row(&row).expect("row maps");
    let query = match proposal.draft {
        asb_core::ccswitch::CcSwitchProviderDraft::Claude(draft) => {
            draft.usage_query.expect("synthesized balance query")
        }
        _ => panic!("expected a Claude proposal"),
    };
    let source = match query {
        UsageQuery::Script { source, .. } => source,
        other => panic!("expected a script query, got {other:?}"),
    };
    let program = ScriptProgram::new(&source).expect("program parses");

    let request = program
        .request("engine-key", Some("https://api.deepseek.com"))
        .expect("request builds");
    assert_eq!(request.url, "https://api.deepseek.com/user/balance");
    assert_eq!(
        request.headers.get("Authorization").map(String::as_str),
        Some("Bearer engine-key")
    );

    let summary = program
        .extract(
            &serde_json::json!({
                "is_available": true,
                "balance_infos": [
                    {"currency": "CNY", "total_balance": "110.00"},
                    {"currency": "USD", "total_balance": 2.5}
                ]
            }),
            200,
            "2026-09-11T22:00:00Z".to_string(),
        )
        .expect("extract builds");
    assert_eq!(summary.readings.len(), 2);
    assert_eq!(summary.readings[0].remaining, Some(110.0));
    assert_eq!(summary.readings[0].unit.as_deref(), Some("CNY"));
    assert_eq!(summary.readings[1].remaining, Some(2.5));
}

/// The vendored coding-plan scripts run end to end in the script engine; the
/// Kimi shape covers the absolute-limit tier projection.
#[test]
fn imported_kimi_token_plan_script_runs_in_the_script_engine() {
    let row = asb_core::ccswitch::CcSwitchRow {
        id: "kimi-1".to_string(),
        app_type: "codex".to_string(),
        name: "Kimi".to_string(),
        settings_config: serde_json::json!({
            "auth": { "OPENAI_API_KEY": "kimi-key" },
            "config": "model_provider = \"custom\"\nmodel = \"kimi-latest\"\n\n[model_providers.custom]\nbase_url = \"https://api.kimi.com/coding\"\nwire_api = \"responses\"\n"
        })
        .to_string(),
        website_url: None,
        notes: None,
        display: None,
        meta: Some(
            r#"{"usage_script":{"enabled":true,"language":"javascript","code":"","templateType":"token_plan","codingPlanProvider":"kimi"}}"#
                .to_string(),
        ),
    };
    let proposal = asb_core::ccswitch::map_row(&row).expect("row maps");
    let query = match proposal.draft {
        asb_core::ccswitch::CcSwitchProviderDraft::Codex(seed) => {
            seed.usage_query.expect("synthesized Kimi query")
        }
        _ => panic!("expected a Codex proposal"),
    };
    let source = match query {
        UsageQuery::Script { source, .. } => source,
        other => panic!("expected a script query, got {other:?}"),
    };
    let program = ScriptProgram::new(&source).expect("program parses");

    let request = program
        .request("kimi-key", Some("https://api.kimi.com/coding"))
        .expect("request builds");
    assert_eq!(request.url, "https://api.kimi.com/coding/v1/usages");
    assert_eq!(
        request.headers.get("Authorization").map(String::as_str),
        Some("Bearer kimi-key")
    );

    let summary = program
        .extract(
            &serde_json::json!({
                "limits": [{ "detail": { "limit": 120.0, "remaining": 90.5, "resetTime": "2026-09-12" } }],
                "usage": { "limit": 900.0, "remaining": 800.0, "resetTime": "2026-09-14" }
            }),
            200,
            "2026-09-11T22:00:00Z".to_string(),
        )
        .expect("extract builds");
    assert_eq!(summary.readings.len(), 2);
    assert_eq!(summary.readings[0].total, Some(120.0));
    assert_eq!(summary.readings[0].remaining, Some(90.5));
    assert_eq!(summary.readings[0].used, Some(29.5));
    assert_eq!(summary.readings[0].plan_name.as_deref(), Some("5 小时窗口"));
    assert_eq!(summary.readings[1].used, Some(100.0));
}
