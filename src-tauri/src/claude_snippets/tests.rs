use super::*;
use asb_core::contracts::{ConfigValue, SettingValue};
use rusqlite::Connection;

fn temp_source(snippet: Option<&str>) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("cc-switch.db");
    let database = Connection::open(&source).unwrap();
    database
        .execute_batch("CREATE TABLE settings(key TEXT, value TEXT);")
        .unwrap();
    if let Some(snippet) = snippet {
        database
            .execute(
                "INSERT INTO settings(key,value) VALUES ('common_config_claude', ?1)",
                [snippet],
            )
            .unwrap();
    }
    database
        .execute(
            "INSERT INTO settings(key,value) VALUES ('common_config_codex', '{}')",
            [],
        )
        .unwrap();
    (dir, source)
}

const SNIPPET: &str = r#"{
        "spinnerTipsEnabled": false,
        "env": {
            "HTTP_PROXY": "http://127.0.0.1:1",
            "ANTHROPIC_AUTH_TOKEN": "source-secret",
            "DISABLE_TELEMETRY": 1
        },
        "permissions": {"allow": ["Read"]},
        "includeCoAuthoredBy": false,
        "mcpServers": {"x": {"command": "keep-out"}}
    }"#;

#[test]
fn scan_reports_the_split_preview_and_named_rejections() {
    let (_dir, source) = temp_source(Some(SNIPPET));
    let scanned = scan(&source).unwrap();
    assert!(scanned.found);
    let preview = scanned.preview.unwrap();
    assert_eq!(
        preview.visual,
        vec![ClaudeSnippetVisualKey {
            key: "spinnerTipsEnabled".into(),
            value: "false".into(),
        }]
    );
    assert_eq!(
        preview.extra_keys,
        vec![
            "/env/DISABLE_TELEMETRY".to_string(),
            "/env/HTTP_PROXY".to_string(),
            "/includeCoAuthoredBy".to_string(),
            "/permissions/allow".to_string(),
        ]
    );
    assert_eq!(
        preview.rejected,
        vec![
            "env.ANTHROPIC_AUTH_TOKEN".to_string(),
            "mcpServers".to_string()
        ]
    );
    let text = serde_json::to_string(&preview).unwrap();
    assert!(!text.contains("source-secret"), "{text}");
}

#[test]
fn scan_reports_a_missing_snippet_without_failing() {
    let (_dir, source) = temp_source(None);
    let scanned = scan(&source).unwrap();
    assert!(!scanned.found);
    assert!(scanned.preview.is_none());
}

#[test]
fn a_missing_settings_table_is_rejected_as_an_unsupported_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("cc-switch.db");
    let database = Connection::open(&source).unwrap();
    database
        .execute_batch("CREATE TABLE providers(id TEXT);")
        .unwrap();
    let error = scan(&source).unwrap_err();
    assert!(error.contains("设置表"), "{error}");
}

#[test]
fn import_merges_visual_and_extra_strips_credentials_and_is_idempotent() {
    let (_dir, source) = temp_source(Some(SNIPPET));
    let store_dir = tempfile::tempdir().unwrap();
    let store = crate::config_store::ConfigStore::new(store_dir.path().into());
    let scanned = scan(&source).unwrap();
    let current = store.get_client_settings(AppKind::Claude).unwrap();

    let result = import(
        &store,
        &source,
        &scanned.source_revision,
        &current.settings_hash,
    )
    .unwrap();
    assert_eq!(result.visual_changed, 1);
    assert_eq!(result.visual_unchanged, 0);
    assert_eq!(result.extra_changed, 4);
    let settings = &result.snapshot.settings;
    assert_eq!(
        settings.settings["spinnerTipsEnabled"],
        SettingValue::Explicit {
            value: ConfigValue::Bool(false)
        }
    );
    assert_eq!(
        settings.claude_extra["env"]["HTTP_PROXY"],
        "http://127.0.0.1:1"
    );
    assert_eq!(settings.claude_extra["env"]["DISABLE_TELEMETRY"], "1");
    assert_eq!(settings.claude_extra["permissions"]["allow"][0], "Read");
    assert!(settings.claude_extra.get("mcpServers").is_none());
    assert!(settings
        .claude_extra
        .get("env")
        .and_then(|env| env.get("ANTHROPIC_AUTH_TOKEN"))
        .is_none());

    let again = import(
        &store,
        &source,
        &scanned.source_revision,
        &result.snapshot.settings_hash,
    )
    .unwrap();
    assert_eq!(again.visual_changed, 0);
    assert_eq!(again.visual_unchanged, 1);
    assert_eq!(again.extra_changed, 0);
}

#[test]
fn import_rejects_stale_source_revisions_and_stale_settings_revisions() {
    let (_dir, source) = temp_source(Some(SNIPPET));
    let store_dir = tempfile::tempdir().unwrap();
    let store = crate::config_store::ConfigStore::new(store_dir.path().into());
    let scanned = scan(&source).unwrap();
    let current = store.get_client_settings(AppKind::Claude).unwrap();
    assert!(
        import(&store, &source, "stale-source", &current.settings_hash)
            .unwrap_err()
            .contains("重新扫描")
    );

    let result = import(
        &store,
        &source,
        &scanned.source_revision,
        &current.settings_hash,
    )
    .unwrap();
    let mut edited = result.snapshot.settings.clone();
    edited
        .settings
        .insert("spinnerTipsEnabled".into(), SettingValue::Automatic);
    store
        .save_client_settings(AppKind::Claude, edited, &result.snapshot.settings_hash)
        .unwrap();
    let error = import(
        &store,
        &source,
        &scanned.source_revision,
        &result.snapshot.settings_hash,
    )
    .unwrap_err();
    assert!(error.contains("重新读取"), "{error}");
}

#[test]
fn an_invalid_choice_value_fails_the_whole_import_with_the_key_named() {
    let snippet = r#"{"preferredNotifChannel": "carrier-pigeon"}"#;
    let (_dir, source) = temp_source(Some(snippet));
    let store_dir = tempfile::tempdir().unwrap();
    let store = crate::config_store::ConfigStore::new(store_dir.path().into());
    let scanned = scan(&source).unwrap();
    let current = store.get_client_settings(AppKind::Claude).unwrap();
    let error = import(
        &store,
        &source,
        &scanned.source_revision,
        &current.settings_hash,
    )
    .unwrap_err();
    assert!(error.contains("preferredNotifChannel"), "{error}");
    // Nothing was written.
    assert_eq!(
        store
            .get_client_settings(AppKind::Claude)
            .unwrap()
            .settings_hash,
        current.settings_hash
    );
}
