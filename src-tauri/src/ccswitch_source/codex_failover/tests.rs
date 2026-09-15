use super::*;
use crate::gateway::codex::policy::CodexGatewayPolicy;
use rusqlite::params;
use std::path::PathBuf;

const TOKEN: &str = "opaque-codex-source-credential-91";
const PROXY_TABLE: &str = "CREATE TABLE proxy_config (app_type TEXT PRIMARY KEY, enabled INTEGER NOT NULL DEFAULT 0, auto_failover_enabled INTEGER NOT NULL DEFAULT 0, max_retries INTEGER NOT NULL DEFAULT 3, streaming_first_byte_timeout INTEGER NOT NULL DEFAULT 60, streaming_idle_timeout INTEGER NOT NULL DEFAULT 120, non_streaming_timeout INTEGER NOT NULL DEFAULT 600, circuit_failure_threshold INTEGER NOT NULL DEFAULT 4, circuit_success_threshold INTEGER NOT NULL DEFAULT 2, circuit_timeout_seconds INTEGER NOT NULL DEFAULT 60, circuit_error_rate_threshold REAL NOT NULL DEFAULT 0.6, circuit_min_requests INTEGER NOT NULL DEFAULT 10)";

struct Fixture {
    dir: tempfile::TempDir,
    path: PathBuf,
}

struct ProxyRow {
    enabled: i64,
    auto: i64,
    retries: i64,
    non_streaming: i64,
    cooldown: i64,
    rate: f64,
}

fn relay_settings(host: &str) -> String {
    serde_json::json!({
        "auth": { "OPENAI_API_KEY": TOKEN },
        "config": format!(
            "model_provider = \"custom\"\nmodel = \"gpt-5-codex\"\n\n[model_providers.custom]\nname = \"relay\"\nbase_url = \"https://{host}\"\nwire_api = \"responses\"\nrequires_openai_auth = true\n"
        ),
    })
    .to_string()
}

fn fixture_db(proxy: Option<ProxyRow>, queue_column: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cc-switch.db");
    let connection = Connection::open(&path).unwrap();
    let queue_columns = if queue_column {
        ", sort_index INTEGER, in_failover_queue BOOLEAN"
    } else {
        ""
    };
    connection
        .execute_batch(&format!(
            "CREATE TABLE providers (id TEXT, app_type TEXT, name TEXT, settings_config TEXT, \
             website_url TEXT, notes TEXT, meta TEXT{queue_columns}, PRIMARY KEY (id, app_type));"
        ))
        .unwrap();
    let official = r#"{"auth":{"OPENAI_API_KEY":null,"tokens":{"refresh_token":"<placeholder>"}},"config":""}"#.to_string();
    for (id, name, settings, index) in [
        ("id-a", "中继 A", relay_settings("a.fixture.invalid"), Some(3)),
        ("id-b", "未导入中继", relay_settings("b.fixture.invalid"), Some(1)),
        ("id-a2", "中继 A 副本", relay_settings("a.fixture.invalid"), Some(4)),
        ("id-official", "官方订阅", official, Some(2)),
    ] {
        if queue_column {
            connection
                .execute(
                    "INSERT INTO providers (id, app_type, name, settings_config, sort_index, in_failover_queue) VALUES (?1, 'codex', ?2, ?3, ?4, 1)",
                    params![id, name, settings, index],
                )
                .unwrap();
        } else {
            connection
                .execute(
                    "INSERT INTO providers (id, app_type, name, settings_config) VALUES (?1, 'codex', ?2, ?3)",
                    params![id, name, settings],
                )
                .unwrap();
        }
    }
    // A queued Claude row must never leak into the Codex proposal.
    if queue_column {
        connection
            .execute(
                "INSERT INTO providers (id, app_type, name, settings_config, sort_index, in_failover_queue) VALUES ('claude-1', 'claude', 'Claude 中继', '{\"env\":{\"ANTHROPIC_BASE_URL\":\"https://c.fixture.invalid\",\"ANTHROPIC_AUTH_TOKEN\":\"x\"}}', 0, 1)",
                [],
            )
            .unwrap();
    }
    if let Some(proxy) = proxy {
        connection.execute_batch(PROXY_TABLE).unwrap();
        connection
            .execute(
                "INSERT INTO proxy_config (app_type, enabled, auto_failover_enabled, max_retries, streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout, circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds, circuit_error_rate_threshold, circuit_min_requests) VALUES ('codex', ?1, ?2, ?3, 45, 90, ?4, 5, 2, ?5, ?6, 12)",
                params![proxy.enabled, proxy.auto, proxy.retries, proxy.non_streaming, proxy.cooldown, proxy.rate],
            )
            .unwrap();
        // The Claude policy row is a different app and must be ignored.
        connection
            .execute(
                "INSERT INTO proxy_config (app_type, enabled, auto_failover_enabled, max_retries) VALUES ('claude', 1, 1, 9)",
                [],
            )
            .unwrap();
    }
    Fixture { dir, path }
}

fn state_with_imported_a(fixture: &Fixture) -> crate::local_state::LocalState {
    let state = crate::local_state::LocalState::from_root(fixture.dir.path().join("state"));
    let outcome =
        crate::ccswitch_source::import_at(&fixture.path, &state, &["codex:id-a".into()]).unwrap();
    assert_eq!(outcome.imported_count, 1);
    state
}

#[test]
fn scan_maps_the_codex_queue_and_policy_with_named_exclusions() {
    let fixture = fixture_db(
        Some(ProxyRow {
            enabled: 1,
            auto: 1,
            retries: 4,
            non_streaming: 0,
            cooldown: 45,
            rate: 0.5,
        }),
        true,
    );
    let state = state_with_imported_a(&fixture);

    let scanned = scan(&fixture.path, &state).unwrap();
    assert!(scanned.found);
    // Source order is sort_index: b(1), official(2), a(3), a-copy(4).
    let names: Vec<_> = scanned
        .members
        .iter()
        .map(|member| member.source_name.as_str())
        .collect();
    assert_eq!(names, ["未导入中继", "官方订阅", "中继 A", "中继 A 副本"]);
    assert_eq!(scanned.members[0].matched_profile_id, None);
    assert_eq!(
        scanned.members[0].endpoint.as_deref(),
        Some("https://b.fixture.invalid/v1")
    );
    assert_eq!(scanned.members[0].upstream, Some(CodexUpstream::Responses));
    assert_eq!(scanned.members[1].endpoint, None);
    let matched = scanned.members[2].matched_profile_id.clone().unwrap();
    assert_eq!(scanned.members[3].matched_profile_id, Some(matched.clone()));
    assert_eq!(scanned.members[2].matched_profile_name.as_deref(), Some("中继 A"));

    let proposal = &scanned.proposal;
    assert_eq!(proposal.provider_ids, vec![matched]);
    assert!(proposal.takeover);
    assert!(proposal.enabled);
    assert_eq!(proposal.max_retries, 4);
    assert_eq!(proposal.traffic.first_byte_timeout_seconds, 45);
    assert_eq!(proposal.traffic.idle_timeout_seconds, 90);
    assert_eq!(
        proposal.traffic.non_streaming_timeout_seconds, 0,
        "来源 0 与本机同为不限制，原样导入"
    );
    assert_eq!(
        proposal.traffic.headers_timeout_seconds,
        CodexGatewayPolicy::default().traffic.headers_timeout_seconds,
        "来源没有响应头阶段，本地值保留"
    );
    assert_eq!(proposal.traffic.failure_threshold, 5);
    assert_eq!(proposal.traffic.success_threshold, 2);
    assert_eq!(proposal.traffic.cooldown_seconds, 45);
    assert_eq!(proposal.traffic.error_rate_percent, 50);
    assert_eq!(proposal.traffic.min_requests, 12);
    assert!(proposal.validate().is_ok());

    let joined = scanned.warnings.join("\n");
    assert!(joined.contains("未导入中继") && joined.contains("请先导入"));
    assert!(joined.contains("官方订阅") && joined.contains("官方登录不参与"));
    assert!(joined.contains("中继 A 副本") && joined.contains("同一本地档案"));
    let serialized = serde_json::to_string(&scanned).unwrap();
    assert!(!serialized.contains(TOKEN));
    assert!(!serialized.contains("Claude 中继"));
}

#[test]
fn out_of_range_values_keep_local_values_and_failover_stays_off_without_takeover() {
    let fixture = fixture_db(
        Some(ProxyRow {
            enabled: 0,
            auto: 1,
            retries: 99,
            non_streaming: 90_000,
            cooldown: 0,
            rate: 0.0,
        }),
        true,
    );
    let state = state_with_imported_a(&fixture);
    let scanned = scan(&fixture.path, &state).unwrap();
    let defaults = CodexGatewayPolicy::default();

    assert!(!scanned.proposal.takeover);
    assert!(!scanned.proposal.enabled, "未接管时不能开启 Failover");
    assert_eq!(scanned.proposal.provider_ids.len(), 1);
    assert_eq!(scanned.proposal.max_retries, defaults.max_retries);
    assert_eq!(
        scanned.proposal.traffic.non_streaming_timeout_seconds,
        defaults.traffic.non_streaming_timeout_seconds
    );
    assert_eq!(
        scanned.proposal.traffic.cooldown_seconds,
        defaults.traffic.cooldown_seconds
    );
    assert_eq!(
        scanned.proposal.traffic.error_rate_percent,
        defaults.traffic.error_rate_percent
    );
    assert_eq!(scanned.proposal.traffic.first_byte_timeout_seconds, 45);
    assert!(scanned.proposal.validate().is_ok());
    let joined = scanned.warnings.join("\n");
    for expected in ["重试次数", "非流式总超时", "熔断冷却时间", "熔断错误率", "未启用接管"] {
        assert!(joined.contains(expected), "缺少告警: {expected}\n{joined}");
    }
}

#[test]
fn a_source_without_codex_failover_data_returns_the_local_policy_untouched() {
    let fixture = fixture_db(None, false);
    let state = state_with_imported_a(&fixture);

    let scanned = scan(&fixture.path, &state).unwrap();
    assert!(!scanned.found);
    assert!(scanned.members.is_empty());
    let (local, revision) = policy::load(state.root()).unwrap();
    assert_eq!(scanned.proposal, local);
    assert_eq!(scanned.policy_revision, revision);
    assert!(scanned.warnings.is_empty());
    assert!(!policy::path(state.root()).exists(), "扫描不得写入本机策略");
}

#[test]
fn source_revision_changes_when_the_queue_or_policy_changes() {
    let fixture = fixture_db(
        Some(ProxyRow {
            enabled: 1,
            auto: 0,
            retries: 2,
            non_streaming: 600,
            cooldown: 30,
            rate: 0.6,
        }),
        true,
    );
    let state = state_with_imported_a(&fixture);
    let before = scan(&fixture.path, &state).unwrap().source_revision;
    Connection::open(&fixture.path)
        .unwrap()
        .execute(
            "UPDATE proxy_config SET max_retries = 3 WHERE app_type = 'codex'",
            [],
        )
        .unwrap();
    assert_ne!(scan(&fixture.path, &state).unwrap().source_revision, before);
}
