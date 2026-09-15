use super::*;
use serde_json::{json, Value};

fn credentials_file(dir: &tempfile::TempDir, text: &str) -> std::path::PathBuf {
    let path = dir.path().join(".credentials.json");
    std::fs::write(&path, text).unwrap();
    path
}

fn serve(status: u16, body: Value) -> (String, std::thread::JoinHandle<Vec<(String, String)>>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}/api/oauth/usage", server.server_addr());
    let task = std::thread::spawn(move || {
        let request = server
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        let headers = request
            .headers()
            .iter()
            .map(|header| {
                (
                    header.field.as_str().as_str().to_ascii_lowercase(),
                    header.value.as_str().to_string(),
                )
            })
            .collect();
        request
            .respond(tiny_http::Response::from_string(body.to_string()).with_status_code(status))
            .unwrap();
        headers
    });
    (base, task)
}

#[test]
fn credential_parsing_accepts_both_key_spellings_and_expiry_forms() {
    let camel =
        parse_credential(r#"{"claudeAiOauth":{"accessToken":"tok","expiresAt":1800000000}}"#)
            .unwrap();
    assert_eq!(camel.access_token, "tok");
    assert_eq!(camel.expires_at_ms, Some(1_800_000_000_000));
    let millis =
        parse_credential(r#"{"claude.ai_oauth":{"accessToken":"tok","expiresAt":1800000000123}}"#)
            .unwrap();
    assert_eq!(millis.expires_at_ms, Some(1_800_000_000_123));
    let iso = parse_credential(
        r#"{"claudeAiOauth":{"accessToken":"tok","expiresAt":"2027-01-01T00:00:00Z"}}"#,
    )
    .unwrap();
    assert_eq!(iso.expires_at_ms, Some(1_798_761_600_000));
    let absent =
        parse_credential(r#"{"claudeAiOauth":{"accessToken":"absent-native-token"}}"#).unwrap();
    assert_eq!(absent.expires_at_ms, None);
    assert!(format!("{absent:?}").contains(asb_core::redact::REDACTED));
    assert!(!format!("{absent:?}").contains("absent-native-token"));
}

#[test]
fn missing_or_invalid_credentials_are_named_without_touching_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join(".credentials.json");
    let error = query(&missing, "http://127.0.0.1:9/unused").unwrap_err();
    assert!(error.contains("未找到"), "{error}");
    assert!(!missing.exists());

    let broken = credentials_file(&dir, "{broken");
    assert!(query(&broken, "http://127.0.0.1:9/unused")
        .unwrap_err()
        .contains("格式无效"));
    assert_eq!(std::fs::read_to_string(&broken).unwrap(), "{broken");

    let no_oauth = credentials_file(&dir, r#"{"other":true}"#);
    assert!(query(&no_oauth, "http://127.0.0.1:9/unused")
        .unwrap_err()
        .contains("没有 claude.ai OAuth"));
    let blank_token = credentials_file(&dir, r#"{"claudeAiOauth":{"accessToken":" "}}"#);
    assert!(query(&blank_token, "http://127.0.0.1:9/unused")
        .unwrap_err()
        .contains("访问令牌"));
}

#[test]
fn a_successful_query_sends_the_oauth_beta_and_reports_known_and_unknown_windows() {
    let dir = tempfile::tempdir().unwrap();
    let path = credentials_file(
        &dir,
        r#"{"claudeAiOauth":{"accessToken":"fixture-token","expiresAt":1000}}"#,
    );
    let (url, task) = serve(
        200,
        json!({
            "five_hour": {"utilization": 12.5, "resets_at": "2026-09-14T10:00:00Z"},
            "seven_day": {"utilization": 40, "resets_at": null},
            "seven_day_opus": {"utilization": null},
            "seven_day_flash": {"utilization": 3.25, "resets_at": "not-a-date"},
            "extra_usage": {"is_enabled": true, "monthly_limit": 50, "used_credits": 12.75, "utilization": 25.5, "currency": "USD"},
            "unrelated": "text"
        }),
    );
    let quota = query(&path, &url).unwrap();
    let headers = task.join().unwrap();
    assert!(headers.contains(&("authorization".into(), "Bearer fixture-token".into())));
    assert!(headers.contains(&("anthropic-beta".into(), OAUTH_BETA.into())));
    assert_eq!(
        quota.windows,
        vec![
            ClaudeNativeQuotaWindow {
                id: "five_hour".into(),
                label: "5 小时".into(),
                used_percent: 12.5,
                resets_at_ms: Some(1_789_380_000_000),
            },
            ClaudeNativeQuotaWindow {
                id: "seven_day".into(),
                label: "7 天".into(),
                used_percent: 40.0,
                resets_at_ms: None,
            },
            ClaudeNativeQuotaWindow {
                id: "seven_day_flash".into(),
                label: "seven_day_flash".into(),
                used_percent: 3.25,
                resets_at_ms: None,
            },
        ]
    );
    assert_eq!(
        quota.extra_usage,
        Some(ClaudeNativeExtraUsage {
            enabled: true,
            monthly_limit: Some(50.0),
            used_credits: Some(12.75),
            utilization: Some(25.5),
            currency: Some("USD".into()),
        })
    );
    assert!(quota.credential_expired);
    assert_eq!(quota.credential_expires_at_ms, Some(1_000_000));
    let serialized = serde_json::to_string(&quota).unwrap();
    assert!(!serialized.contains("fixture-token"));
}

#[test]
fn rejected_authorization_and_invalid_bodies_are_named_without_the_token() {
    let dir = tempfile::tempdir().unwrap();
    let path = credentials_file(&dir, r#"{"claudeAiOauth":{"accessToken":"fixture-token"}}"#);
    let (url, task) = serve(401, json!({"error": "unauthorized"}));
    let error = query(&path, &url).unwrap_err();
    task.join().unwrap();
    assert!(error.contains("重新登录"), "{error}");
    assert!(!error.contains("fixture-token"));

    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/oauth/usage", server.server_addr());
    let task = std::thread::spawn(move || {
        let request = server
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        request
            .respond(tiny_http::Response::from_string("not json"))
            .unwrap();
    });
    let error = query(&path, &url).unwrap_err();
    task.join().unwrap();
    assert!(error.contains("不是有效 JSON"), "{error}");
}
