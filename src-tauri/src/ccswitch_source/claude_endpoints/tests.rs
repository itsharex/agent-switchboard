use super::*;
use crate::local_state::LocalState;
use rusqlite::{params, Connection};
use std::path::PathBuf;

const TOKEN: &str = "opaque-source-credential-99";
const ENDPOINTS_TABLE: &str = "CREATE TABLE provider_endpoints (provider_id TEXT NOT NULL, app_type TEXT NOT NULL, url TEXT NOT NULL, added_at INTEGER, PRIMARY KEY (provider_id, app_type, url))";

struct Fixture {
    dir: tempfile::TempDir,
    path: PathBuf,
}

fn fixture_db(with_table: bool, meta_endpoints: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cc-switch.db");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE providers (id TEXT, app_type TEXT, name TEXT, settings_config TEXT, \
             website_url TEXT, notes TEXT, meta TEXT, PRIMARY KEY (id, app_type));",
        )
        .unwrap();
    let meta = if meta_endpoints {
        r#"{"custom_endpoints":{"https://meta.fixture.invalid":{"url":"https://meta.fixture.invalid","addedAt":111}}}"#
    } else {
        "{}"
    };
    let settings = format!(
        r#"{{"env":{{"ANTHROPIC_BASE_URL":"https://relay.fixture.invalid","ANTHROPIC_AUTH_TOKEN":"{TOKEN}","ANTHROPIC_MODEL":"claude-x"}}}}"#
    );
    connection
        .execute(
            "INSERT INTO providers (id, app_type, name, settings_config, meta) VALUES ('id-a', 'claude', '中继 A', ?1, ?2)",
            params![settings, meta],
        )
        .unwrap();
    if with_table {
        connection.execute_batch(ENDPOINTS_TABLE).unwrap();
        for (url, added_at) in [
            ("https://fast.fixture.invalid", 222),
            ("https://fast2.fixture.invalid", 111),
        ] {
            connection
                .execute(
                    "INSERT INTO provider_endpoints (provider_id, app_type, url, added_at) VALUES ('id-a', 'claude', ?1, ?2)",
                    params![url, added_at],
                )
                .unwrap();
        }
    }
    Fixture { dir, path }
}

fn claude_item<'a>(
    scanned: &'a crate::ccswitch_source::CcSwitchScan,
) -> &'a crate::ccswitch_source::CcSwitchScanItem {
    scanned
        .providers
        .iter()
        .find(|item| item.key == "claude:id-a")
        .unwrap()
}

#[test]
fn table_candidates_replace_meta_and_persist_with_the_profile() {
    let fixture = fixture_db(true, true);
    let state = LocalState::from_root(fixture.dir.path().join("state"));

    let scanned = crate::ccswitch_source::scan_at(&fixture.path, &state).unwrap();
    assert_eq!(claude_item(&scanned).endpoint_candidates, 2);

    let outcome =
        crate::ccswitch_source::import_at(&fixture.path, &state, &["claude:id-a".into()]).unwrap();
    assert_eq!(outcome.endpoint_candidates_imported, 2);
    let mut records = state.configuration().list_providers().unwrap();
    let endpoints = records.remove(0).profile.connection.custom_endpoints;
    assert_eq!(endpoints.len(), 2);
    assert_eq!(endpoints["https://fast.fixture.invalid"].added_at, 222);
    assert_eq!(endpoints["https://fast2.fixture.invalid"].added_at, 111);
    assert!(!endpoints.contains_key("https://meta.fixture.invalid"));
    assert!(!serde_json::to_string(&outcome).unwrap().contains(TOKEN));

    let again =
        crate::ccswitch_source::import_at(&fixture.path, &state, &["claude:id-a".into()]).unwrap();
    assert_eq!(again.imported_count, 0);
    assert_eq!(again.endpoint_candidates_imported, 0);
}

#[test]
fn without_the_table_meta_candidates_survive() {
    let fixture = fixture_db(false, true);
    let state = LocalState::from_root(fixture.dir.path().join("state"));

    let scanned = crate::ccswitch_source::scan_at(&fixture.path, &state).unwrap();
    assert_eq!(claude_item(&scanned).endpoint_candidates, 1);

    let outcome =
        crate::ccswitch_source::import_at(&fixture.path, &state, &["claude:id-a".into()]).unwrap();
    assert_eq!(outcome.endpoint_candidates_imported, 1);
    let mut records = state.configuration().list_providers().unwrap();
    let endpoints = records.remove(0).profile.connection.custom_endpoints;
    assert_eq!(endpoints["https://meta.fixture.invalid"].added_at, 111);
}

#[test]
fn a_source_without_candidates_imports_none() {
    let fixture = fixture_db(false, false);
    let state = LocalState::from_root(fixture.dir.path().join("state"));

    let scanned = crate::ccswitch_source::scan_at(&fixture.path, &state).unwrap();
    assert_eq!(claude_item(&scanned).endpoint_candidates, 0);

    let outcome =
        crate::ccswitch_source::import_at(&fixture.path, &state, &["claude:id-a".into()]).unwrap();
    assert_eq!(outcome.imported_count, 1);
    assert_eq!(outcome.endpoint_candidates_imported, 0);
}
