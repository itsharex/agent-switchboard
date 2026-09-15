use super::*;
use crate::gateway::failover;
use rusqlite::params;
use std::path::PathBuf;

const TOKEN: &str = "opaque-source-credential-77";
const PROXY_TABLE: &str = "CREATE TABLE proxy_config (app_type TEXT PRIMARY KEY, enabled INTEGER NOT NULL DEFAULT 0, auto_failover_enabled INTEGER NOT NULL DEFAULT 0, max_retries INTEGER NOT NULL DEFAULT 3, streaming_first_byte_timeout INTEGER NOT NULL DEFAULT 60, streaming_idle_timeout INTEGER NOT NULL DEFAULT 120, non_streaming_timeout INTEGER NOT NULL DEFAULT 600, circuit_failure_threshold INTEGER NOT NULL DEFAULT 4, circuit_success_threshold INTEGER NOT NULL DEFAULT 2, circuit_timeout_seconds INTEGER NOT NULL DEFAULT 60, circuit_error_rate_threshold REAL NOT NULL DEFAULT 0.6, circuit_min_requests INTEGER NOT NULL DEFAULT 10)";

struct Fixture {
    dir: tempfile::TempDir,
    path: PathBuf,
}

fn fixture_db(with_proxy: bool, queue_column: bool, rate: f64, retries: i64) -> Fixture {
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
    for (id, name, queued, index) in [
        ("id-a", "中继 A", true, Some(2)),
        ("id-b", "未导入中继", true, Some(1)),
    ] {
        let settings = format!(
            r#"{{"env":{{"ANTHROPIC_BASE_URL":"https://{id}.fixture.invalid","ANTHROPIC_AUTH_TOKEN":"{TOKEN}","ANTHROPIC_MODEL":"claude-x"}}}}"#
        );
        if queue_column {
            connection
                .execute(
                    "INSERT INTO providers (id, app_type, name, settings_config, sort_index, in_failover_queue) VALUES (?1, 'claude', ?2, ?3, ?4, ?5)",
                    params![id, name, settings, index, queued],
                )
                .unwrap();
        } else {
            connection
                .execute(
                    "INSERT INTO providers (id, app_type, name, settings_config) VALUES (?1, 'claude', ?2, ?3)",
                    params![id, name, settings],
                )
                .unwrap();
        }
    }
    if with_proxy {
        connection.execute_batch(PROXY_TABLE).unwrap();
        connection
            .execute(
                "INSERT INTO proxy_config (app_type, enabled, auto_failover_enabled, max_retries, streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout, circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds, circuit_error_rate_threshold, circuit_min_requests) VALUES ('claude', 1, 1, ?1, 90, 180, 600, 8, 3, 90, ?2, 15)",
                params![retries, rate],
            )
            .unwrap();
    }
    Fixture { dir, path }
}

fn state_with_imported_a(fixture: &Fixture) -> crate::local_state::LocalState {
    let state = crate::local_state::LocalState::from_root(fixture.dir.path().join("state"));
    let outcome =
        crate::ccswitch_source::import_at(&fixture.path, &state, &["claude:id-a".into()]).unwrap();
    assert_eq!(outcome.imported_count, 1);
    state
}

#[test]
fn scan_maps_queue_members_and_policy_with_a_named_unmatched_member() {
    let fixture = fixture_db(true, true, 0.7, 6);
    let state = state_with_imported_a(&fixture);

    let scanned = scan(&fixture.path, &state).unwrap();
    assert!(scanned.found);
    assert_eq!(scanned.members.len(), 2);
    // Source order is sort_index: id-b (1) before id-a (2); only the imported
    // row has a local routing match and enters the proposed queue.
    assert_eq!(scanned.members[0].source_name, "未导入中继");
    assert_eq!(scanned.members[0].matched_profile_id, None);
    assert_eq!(scanned.members[1].source_name, "中继 A");
    let matched_id = scanned.members[1].matched_profile_id.clone().unwrap();
    let stored = state.configuration().find_provider(&matched_id).unwrap();
    assert_eq!(
        stored.base_url.as_deref(),
        Some("https://id-a.fixture.invalid")
    );
    assert_eq!(scanned.proposal.provider_ids, vec![matched_id]);
    assert!(scanned.proposal.enabled);
    assert!(scanned.proposal.takeover);
    assert_eq!(scanned.proposal.max_retries, 6);
    assert_eq!(
        scanned
            .proposal
            .traffic
            .streaming_first_byte_timeout_seconds,
        90
    );
    assert_eq!(scanned.proposal.traffic.circuit_failure_threshold, 8);
    assert_eq!(scanned.proposal.traffic.circuit_error_rate_percent, 70);
    assert!(scanned
        .warnings
        .iter()
        .any(|warning| warning.contains("未导入中继") && warning.contains("请先导入")));
    assert!(!serde_json::to_string(&scanned).unwrap().contains(TOKEN));
}

#[test]
fn confirmed_import_persists_the_proposed_policy_and_rejects_stale_revisions() {
    let fixture = fixture_db(true, true, 0.7, 6);
    let state = state_with_imported_a(&fixture);
    let scanned = scan(&fixture.path, &state).unwrap();

    assert!(
        import(&fixture.path, "stale", &scanned.policy_revision, &state)
            .unwrap_err()
            .contains("重新扫描")
    );
    let (policy, warnings) = import(
        &fixture.path,
        &scanned.source_revision,
        &scanned.policy_revision,
        &state,
    )
    .unwrap();
    assert_eq!(policy, scanned.proposal);
    assert_eq!(policy.provider_ids.len(), 1);
    failover::save(state.root(), &policy).unwrap();
    assert_eq!(failover::load(state.root()).unwrap(), policy);
    assert!(warnings
        .iter()
        .any(|warning| warning.contains("未导入中继")));

    assert!(import(
        &fixture.path,
        &scanned.source_revision,
        "stale-policy",
        &state
    )
    .unwrap_err()
    .contains("重新读取"));
}

#[test]
fn out_of_range_source_values_keep_the_local_policy_and_name_the_field() {
    let fixture = fixture_db(true, true, 0.0, 99);
    Connection::open(&fixture.path)
        .unwrap()
        .execute(
            "UPDATE proxy_config SET streaming_first_byte_timeout = 0 WHERE app_type = 'claude'",
            [],
        )
        .unwrap();
    let state = state_with_imported_a(&fixture);
    let scanned = scan(&fixture.path, &state).unwrap();

    // In-range neighbors still import; out-of-range values keep the local
    // policy and name the field instead of clamping into fake meaning.
    assert!(scanned.proposal.enabled);
    assert_eq!(scanned.proposal.max_retries, 0);
    assert_eq!(
        scanned
            .proposal
            .traffic
            .streaming_first_byte_timeout_seconds,
        failover::ClaudeFailoverPolicy::default()
            .traffic
            .streaming_first_byte_timeout_seconds
    );
    assert_eq!(
        scanned.proposal.traffic.circuit_error_rate_percent,
        failover::ClaudeFailoverPolicy::default()
            .traffic
            .circuit_error_rate_percent
    );
    assert!(scanned
        .warnings
        .iter()
        .any(|warning| warning.contains("重试次数")));
    assert!(scanned
        .warnings
        .iter()
        .any(|warning| warning.contains("流式首包超时")));
    assert!(scanned
        .warnings
        .iter()
        .any(|warning| warning.contains("熔断错误率")));
}

#[test]
fn a_source_without_failover_data_leaves_the_local_policy_untouched() {
    let fixture = fixture_db(false, false, 0.6, 3);
    let state = state_with_imported_a(&fixture);

    let scanned = scan(&fixture.path, &state).unwrap();
    assert!(!scanned.found);
    assert!(scanned.members.is_empty());
    assert_eq!(scanned.proposal, failover::load(state.root()).unwrap());
    assert!(scanned.warnings.is_empty());

    let (policy, warnings) = import(
        &fixture.path,
        &scanned.source_revision,
        &scanned.policy_revision,
        &state,
    )
    .unwrap();
    assert_eq!(policy, failover::load(state.root()).unwrap());
    assert!(warnings.is_empty());
}
