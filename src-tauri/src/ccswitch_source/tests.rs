use super::db::open_read_only;
use super::*;
use asb_core::contracts::{
    AuthenticationScheme, ClaudeModelSettings, ModelOptions, ProviderDraft, ProviderProfile,
    UpstreamProtocol,
};
use rusqlite::{params, Connection};
use std::path::PathBuf;

fn profiles(state: &LocalState) -> Vec<ProviderProfile> {
    state
        .configuration()
        .list_providers()
        .unwrap()
        .into_iter()
        .map(|record| record.profile)
        .collect()
}

// A deliberately nonstandard credential shape proves scan serialization
// cannot rely on matching a token prefix to keep credentials private.
const SOURCE_TOKEN: &str = "opaque-source-credential-42";
const SOURCE_CODEX_TOKEN: &str = "opaque-codex-credential-84";
const SOURCE_USAGE_SCRIPT: &str = r#"({
        request: {
            url: "{{baseUrl}}/usage",
            method: "GET",
            headers: { Authorization: "Bearer {{apiKey}}" }
        },
        extractor: function(response) {
            return {
                isValid: true,
                planName: "主套餐",
                remaining: response.remaining,
                used: response.used,
                total: 100,
                unit: "USD"
            };
        }
    })"#;

fn fixture_db(dir: &Path) -> PathBuf {
    let path = dir.join("cc-switch.db");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE providers (
                    id TEXT,
                    app_type TEXT,
                    name TEXT,
                    settings_config TEXT,
                    website_url TEXT,
                    notes TEXT,
                    meta TEXT,
                    PRIMARY KEY (id, app_type)
                );",
        )
        .unwrap();
    connection
            .execute(
                "INSERT INTO providers (id, app_type, name, settings_config, website_url, notes, meta) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    "id-1",
                    "claude",
                    "中继 A",
                    format!(
                        r#"{{"env":{{"ANTHROPIC_BASE_URL":"https://relay.internal","ANTHROPIC_AUTH_TOKEN":"{SOURCE_TOKEN}","ANTHROPIC_MODEL":"claude-x","CLAUDE_CODE_SUBAGENT_MODEL":"sub-x"}},"permissions":{{"defaultMode":"auto"}}}}"#
                    ),
                    "https://relay.internal",
                    "主力中继",
                    serde_json::json!({
                        "usage_script": {
                            "enabled": true,
                            "language": "javascript",
                            "code": SOURCE_USAGE_SCRIPT,
                            "timeout": 8
                        }
                    })
                    .to_string(),
                ],
            )
            .unwrap();
    connection
            .execute(
                "INSERT INTO providers (id, app_type, name, settings_config) VALUES (?1, ?2, ?3, ?4)",
                params![
                    "id-2",
                    "codex",
                    "订阅",
                    r#"{"auth":{"OPENAI_API_KEY":null,"tokens":{"refresh_token":"<placeholder>"}},"config":""}"#
                ],
            )
            .unwrap();
    connection
        .execute(
            "INSERT INTO providers (id, app_type, name, settings_config) VALUES (?1, ?2, ?3, ?4)",
            params!["id-3", "gemini", "双子", "{}"],
        )
        .unwrap();
    path
}

/// A Codex row in the shape the source itself writes: credential in `auth`,
/// an optional real-shaped model catalog, and metadata keys this app does
/// not consume.
fn insert_real_codex_with_meta(connection: &Connection, id: &str, meta: String) {
    let config = r#"model_provider = "custom"
model = "gpt-5-codex"

[model_providers.custom]
name = "Codex 中继"
base_url = "https://relay.codex.example"
wire_api = "responses"
requires_openai_auth = true
"#;
    let settings = serde_json::json!({
        "auth": { "OPENAI_API_KEY": SOURCE_CODEX_TOKEN },
        "config": config,
        "modelCatalog": {
            "models": [
                {
                    "model": "gpt-5-codex",
                    "displayName": "GPT-5 Codex",
                    "contextWindow": 272000,
                    "inputModalities": ["text"]
                },
                { "model": "gpt-5-codex-mini", "contextWindow": "200000" }
            ]
        }
    });
    connection
        .execute(
            "INSERT INTO providers (id, app_type, name, settings_config, meta) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, "codex", "Codex 中继", settings.to_string(), meta],
        )
        .unwrap();
}

fn insert_real_codex(connection: &Connection, id: &str) {
    insert_real_codex_with_meta(
        connection,
        id,
        serde_json::json!({
            "apiFormat": "openai_responses",
            "costMultiplier": 0.9
        })
        .to_string(),
    );
}

/// Same route as [`insert_real_codex`] plus an enabled usage script in meta.
fn insert_real_codex_with_usage_script(connection: &Connection, id: &str) {
    insert_real_codex_with_meta(
        connection,
        id,
        serde_json::json!({
            "usage_script": {
                "enabled": true,
                "language": "javascript",
                "code": SOURCE_USAGE_SCRIPT,
                "timeout": 8
            }
        })
        .to_string(),
    );
}

#[test]
fn scan_marks_duplicates_and_filters_out_of_scope_clients() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_db(dir.path());
    let state = LocalState::from_root(dir.path().join("state"));

    let first = scan_at(&path, &state).unwrap();
    assert_eq!(first.db_path, path.to_string_lossy());
    // The response carries only actionable rows: the Claude relay and the
    // Codex official record. The gemini row is invisible, and no skip wall
    // exists for the official row anymore.
    assert_eq!(first.providers.len(), 2);
    assert!(first.skipped.is_empty());
    let official = first
        .providers
        .iter()
        .find(|item| item.key == "codex:id-2")
        .expect("official Codex row");
    assert_eq!(official.app, AppKind::Codex);
    assert_eq!(official.route_mode, asb_core::RouteMode::Official);
    assert_eq!(official.name, "Codex 官方登录");
    assert!(official.base_url.is_none());
    assert!(!official.existing);
    assert!(first.providers.iter().all(|item| !item.existing));
    let serialized = serde_json::to_string(&first).unwrap();
    assert!(!serialized.contains(SOURCE_TOKEN));
    assert!(!serialized.contains(SOURCE_USAGE_SCRIPT));
    assert!(!serialized.contains("\"apiKey\""));
    assert!(!serialized.contains("\"draft\""));

    let claude = first
        .providers
        .iter()
        .find(|item| item.app == AppKind::Claude)
        .expect("claude proposal");
    assert_eq!(claude.name, "中继 A");
    assert_eq!(claude.model.as_deref(), Some("claude-x"));
    assert_eq!(claude.base_url.as_deref(), Some("https://relay.internal"));
    assert!(claude.usage_script_importable);
    let imported = import_at(&path, &state, &[claude.key.clone()]).unwrap();
    assert_eq!(imported.imported_count, 1);
    assert!(!serde_json::to_string(&imported)
        .unwrap()
        .contains(SOURCE_TOKEN));
    assert!(!serde_json::to_string(&imported)
        .unwrap()
        .contains(SOURCE_USAGE_SCRIPT));
    let mut profiles = profiles(&state);
    let profile = profiles.remove(0);
    assert_eq!(profile.api_key, SOURCE_TOKEN);
    assert!(matches!(
        profile.usage_query,
        Some(asb_core::contracts::UsageQuery::Script { .. })
    ));
    assert_eq!(imported.usage_script_imported_count, 1);
    let second = scan_at(&path, &state).unwrap();
    assert!(
        second
            .providers
            .iter()
            .find(|item| item.key == claude.key)
            .expect("same proposal")
            .existing
    );
}

#[test]
fn import_reuses_dedup_and_reports_skips() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_db(dir.path());
    let state = LocalState::from_root(dir.path().join("state"));

    let outcome = import_at(
        &path,
        &state,
        &[
            "claude:id-1".into(),
            "gemini:id-3".into(),
            "claude:id-9".into(),
        ],
    )
    .unwrap();
    assert_eq!(outcome.imported_count, 1);
    assert_eq!(outcome.not_imported.len(), 2);
    assert!(outcome
        .not_imported
        .iter()
        .any(|item| item.reason.contains("gemini")));
    assert!(!serde_json::to_string(&outcome)
        .unwrap()
        .contains(SOURCE_TOKEN));

    let again = import_at(&path, &state, &["claude:id-1".into()]).unwrap();
    assert_eq!(again.imported_count, 0);
    assert_eq!(again.skipped_existing, vec!["中继 A".to_string()]);
}

#[test]
fn codex_rows_import_in_one_click_with_backend_completion() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_db(dir.path());
    let connection = Connection::open(&path).unwrap();
    insert_real_codex(&connection, "id-codex");
    let state = LocalState::from_root(dir.path().join("state"));

    let first = scan_at(&path, &state).unwrap();
    let codex = first
        .providers
        .iter()
        .find(|item| item.key == "codex:id-codex")
        .expect("Codex row");
    assert_eq!(codex.app, AppKind::Codex);
    assert_eq!(codex.model.as_deref(), Some("gpt-5-codex"));
    assert_eq!(
        codex.base_url.as_deref(),
        Some("https://relay.codex.example/v1")
    );
    assert!(!codex.existing);
    assert!(codex
        .warnings
        .iter()
        .any(|warning| warning.contains("costMultiplier")));
    assert!(!codex
        .warnings
        .iter()
        .any(|warning| warning.contains("displayName")));
    let serialized = serde_json::to_string(&first).unwrap();
    assert!(!serialized.contains(SOURCE_CODEX_TOKEN));
    assert!(!serialized.contains("\"apiKey\""));
    assert!(!serialized.contains("\"draft\""));

    // One mixed click imports both stores at once: Claude into the generic
    // store and the completed Codex row into the strict store.
    let outcome = import_at(
        &path,
        &state,
        &["claude:id-1".into(), "codex:id-codex".into()],
    )
    .unwrap();
    assert_eq!(outcome.imported_count, 2);
    assert_eq!(outcome.usage_script_imported_count, 1);
    assert!(!serde_json::to_string(&outcome)
        .unwrap()
        .contains(SOURCE_CODEX_TOKEN));

    let codex_records = state.configuration().list_codex_providers().unwrap();
    assert_eq!(codex_records.len(), 1);
    assert_eq!(codex_records[0].profile.name, "Codex 中继");
    assert_eq!(codex_records[0].profile.api_key, SOURCE_CODEX_TOKEN);
    assert_eq!(
        codex_records[0].profile.endpoint.0,
        "https://relay.codex.example/v1"
    );
    assert_eq!(codex_records[0].profile.catalog.len(), 2);
    assert_eq!(codex_records[0].profile.catalog[0].context_window, 272_000);
    assert_eq!(
        codex_records[0].profile.catalog[0].display_name.as_deref(),
        Some("GPT-5 Codex")
    );

    // The same route never imports twice.
    let again = import_at(&path, &state, &["codex:id-codex".into()]).unwrap();
    assert_eq!(again.imported_count, 0);
    assert_eq!(again.skipped_existing, vec!["Codex 中继".to_string()]);

    let second = scan_at(&path, &state).unwrap();
    assert!(
        second
            .providers
            .iter()
            .find(|item| item.key == "codex:id-codex")
            .expect("same Codex row")
            .existing
    );
}

#[test]
fn codex_official_row_imports_into_the_generic_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_db(dir.path());
    let state = LocalState::from_root(dir.path().join("state"));

    let outcome = import_at(&path, &state, &["codex:id-2".into()]).unwrap();
    assert_eq!(outcome.imported_count, 1);
    let official = state
        .configuration()
        .list_providers()
        .unwrap()
        .into_iter()
        .find(|record| record.profile.app == AppKind::Codex)
        .expect("official Codex record");
    assert_eq!(official.profile.route_mode, asb_core::RouteMode::Official);
    assert_eq!(official.profile.name, "Codex 官方登录");
    assert!(official.profile.api_key.is_empty());

    // The single official record never duplicates.
    let again = import_at(&path, &state, &["codex:id-2".into()]).unwrap();
    assert_eq!(again.imported_count, 0);
    assert_eq!(again.skipped_existing, vec!["Codex 官方登录".to_string()]);
    let second = scan_at(&path, &state).unwrap();
    assert!(
        second
            .providers
            .iter()
            .find(|item| item.key == "codex:id-2")
            .expect("official row")
            .existing
    );
}

#[test]
fn import_enriches_a_matching_profile_with_the_source_usage_script() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_db(dir.path());
    let state = LocalState::from_root(dir.path().join("state"));
    let existing = state
        .configuration()
        .import_provider(ProviderDraft {
            authentication: Some(AuthenticationScheme::Bearer),
            parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
            app: AppKind::Claude,
            route_mode: asb_core::RouteMode::Custom,
            name: "中继 A".to_string(),
            model: Some("claude-x".to_string()),
            base_url: Some("https://relay.internal".to_string()),
            connection: Default::default(),
            api_key: SOURCE_TOKEN.to_string(),
            upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
            responses_options: None,
            max_output_tokens: None.into(),
            model_options: Some(ModelOptions::Claude(ClaudeModelSettings {
                haiku_one_m: false,
                subagent_model: Some("sub-x".to_string()),
                ..ClaudeModelSettings::default()
            })),
            notes: Some("主力中继".to_string()),
            website_url: Some("https://relay.internal".to_string()),
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        })
        .expect("existing routing profile");

    let scan = scan_at(&path, &state).expect("scan");
    let proposal = scan
        .providers
        .iter()
        .find(|provider| provider.key == "claude:id-1")
        .expect("custom provider");
    assert!(!proposal.existing);
    assert!(proposal.usage_script_importable);
    assert!(proposal.usage_script_updates_existing);

    let outcome = import_at(&path, &state, &["claude:id-1".to_string()]).expect("import");
    assert_eq!(outcome.imported_count, 1);
    assert_eq!(outcome.usage_script_imported_count, 1);
    let profiles = profiles(&state);
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].id, existing.profile.id);
    assert!(profiles[0].usage_query.is_some());
}

#[test]
fn import_persists_lowercase_one_m_as_semantic_profile_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_db(dir.path());
    let connection = Connection::open(&path).unwrap();
    connection
            .execute(
                "UPDATE providers SET settings_config = ?1 WHERE id = ?2 AND app_type = ?3",
                params![
                    r#"{"env":{"ANTHROPIC_BASE_URL":"https://relay.internal","ANTHROPIC_AUTH_TOKEN":"<placeholder>","ANTHROPIC_MODEL":"claude-opus-4-1[1m]","ANTHROPIC_DEFAULT_SONNET_MODEL":"claude-sonnet-4-6[1m]","ANTHROPIC_DEFAULT_OPUS_MODEL":"claude-opus-4-1[1m]"}}"#,
                    "id-1",
                    "claude",
                ],
            )
            .unwrap();

    let state = LocalState::from_root(dir.path().join("state"));
    let outcome = import_at(&path, &state, &["claude:id-1".into()]).unwrap();
    assert_eq!(outcome.imported_count, 1);
    let profiles = profiles(&state);
    let imported = profiles.first().expect("one profile should import");

    assert_eq!(imported.model.as_deref(), Some("claude-opus-4-1"));
    let Some(asb_core::ModelOptions::Claude(settings)) = imported.model_options.as_ref() else {
        panic!("Claude model settings should be persisted");
    };
    assert!(settings.primary_one_m);
    assert_eq!(settings.sonnet_model.as_deref(), Some("claude-sonnet-4-6"));
    assert!(settings.sonnet_one_m);
    assert_eq!(settings.opus_model.as_deref(), Some("claude-opus-4-1"));
    assert!(settings.opus_one_m);
}

#[test]
fn import_normalizes_ccswitch_uppercase_one_m_models_before_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_db(dir.path());
    let connection = Connection::open(&path).unwrap();
    connection
            .execute(
                "UPDATE providers SET settings_config = ?1 WHERE id = ?2 AND app_type = ?3",
                params![
                    r#"{"env":{"ANTHROPIC_BASE_URL":"https://relay.internal","ANTHROPIC_AUTH_TOKEN":"<placeholder>","ANTHROPIC_MODEL":"claude-opus-4-1[1M]"}}"#,
                    "id-1",
                    "claude",
                ],
            )
            .unwrap();

    let state = LocalState::from_root(dir.path().join("state"));
    let scan = scan_at(&path, &state).unwrap();
    assert!(scan.providers.iter().any(|item| item.key == "claude:id-1"));

    let outcome = import_at(&path, &state, &["claude:id-1".into()]).unwrap();
    assert_eq!(outcome.imported_count, 1);
    let profiles = profiles(&state);
    let Some(asb_core::ModelOptions::Claude(settings)) = profiles[0].model_options.as_ref() else {
        panic!("Claude model settings should be persisted");
    };
    assert!(settings.primary_one_m);
}

#[test]
fn missing_database_reports_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let error = open_read_only(&dir.path().join("none.db")).unwrap_err();
    assert!(error.contains("未找到"));
}

/// Read-only smoke test against the user's real source database.
/// Ignored by default: it needs a machine that actually has source data;
/// run explicitly with `cargo test -p agent-switchboard -- --ignored`.
///
/// The serialized scan contract has no API-key field. Its warnings carry
/// only unsupported field names, never values.
#[test]
#[ignore = "requires a real external provider database under the user profile"]
fn real_database_scan_is_read_only_and_secret_free() {
    let dir = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(dir.path().join("state"));
    let scan = scan(&state).expect("real scan");
    assert!(scan.db_path.ends_with("cc-switch.db"));
    let serialized = serde_json::to_string(&scan).unwrap();
    for item in &scan.providers {
        for warning in &item.warnings {
            assert!(warning.starts_with("未导入: "));
        }
        assert!(item.base_url.as_deref().unwrap_or("").len() > 8);
        assert!(!item.name.contains(':'));
    }
    eprintln!(
        "real scan: {} providers, {} skipped",
        scan.providers.len(),
        scan.skipped.len()
    );
    assert!(!serialized.contains("\"apiKey\""));
    assert!(!serialized.contains("\"draft\""));
}

/// The strict store mirrors the generic import boundary's enrichment: a
/// route-identical Codex row delivers its usage query to the stored profile
/// instead of skipping, and only a true duplicate skips.
#[test]
fn codex_route_import_enriches_a_missing_usage_query() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_db(dir.path());
    let connection = Connection::open(&path).unwrap();
    insert_real_codex(&connection, "id-plain");
    insert_real_codex_with_usage_script(&connection, "id-scripted");
    let state = LocalState::from_root(dir.path().join("state"));

    // After the plain row creates the profile, the scripted same-route row
    // is an enrichment candidate, not a duplicate.
    let created = import_at(&path, &state, &["codex:id-plain".into()]).unwrap();
    assert_eq!(created.imported_count, 1);
    assert_eq!(created.usage_script_imported_count, 0);

    let scan = scan_at(&path, &state).unwrap();
    let scripted = scan
        .providers
        .iter()
        .find(|item| item.key == "codex:id-scripted")
        .expect("scripted row");
    assert!(scripted.usage_script_updates_existing);
    assert!(!scripted.existing);

    let enriched = import_at(&path, &state, &["codex:id-scripted".into()]).unwrap();
    assert_eq!(enriched.imported_count, 1);
    assert_eq!(enriched.usage_script_imported_count, 1);
    assert!(enriched.skipped_existing.is_empty());
    let records = state.configuration().list_codex_providers().unwrap();
    assert_eq!(records.len(), 1);
    assert!(matches!(
        records[0].usage_query,
        Some(asb_core::contracts::UsageQuery::Script { .. })
    ));

    // The delivered query makes the row a true duplicate from now on.
    let again = scan_at(&path, &state).unwrap();
    let scripted = again
        .providers
        .iter()
        .find(|item| item.key == "codex:id-scripted")
        .expect("scripted row");
    assert!(scripted.existing);
    assert!(!scripted.usage_script_updates_existing);
    let repeated = import_at(&path, &state, &["codex:id-scripted".into()]).unwrap();
    assert_eq!(repeated.imported_count, 0);
    assert_eq!(repeated.skipped_existing, vec!["Codex 中继".to_string()]);
}

/// The one-click create path persists the source usage script together with
/// the completed strict profile.
#[test]
fn codex_one_click_create_persists_the_source_usage_script() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_db(dir.path());
    let connection = Connection::open(&path).unwrap();
    insert_real_codex_with_usage_script(&connection, "id-scripted");
    let state = LocalState::from_root(dir.path().join("state"));

    let outcome = import_at(&path, &state, &["codex:id-scripted".into()]).unwrap();
    assert_eq!(outcome.imported_count, 1);
    assert_eq!(outcome.usage_script_imported_count, 1);
    let records = state.configuration().list_codex_providers().unwrap();
    assert_eq!(records.len(), 1);
    assert!(matches!(
        records[0].usage_query,
        Some(asb_core::contracts::UsageQuery::Script { .. })
    ));
}

/// A Claude row whose usage script is the built-in balance template (empty
/// code) imports with the synthesized native query; the store's persisted
/// validation accepting it is the proof that the generated program is a
/// valid native script.
#[test]
fn claude_balance_template_row_imports_with_a_native_usage_query() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_db(dir.path());
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "INSERT INTO providers (id, app_type, name, settings_config, meta) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                "id-deepseek",
                "claude",
                "DeepSeek",
                format!(
                    r#"{{"env":{{"ANTHROPIC_BASE_URL":"https://api.deepseek.com","ANTHROPIC_AUTH_TOKEN":"{SOURCE_TOKEN}","ANTHROPIC_MODEL":"deepseek-chat"}}}}"#
                ),
                serde_json::json!({
                    "usage_script": {
                        "enabled": true,
                        "language": "javascript",
                        "code": "",
                        "templateType": "balance",
                        "autoQueryInterval": 30
                    }
                })
                .to_string(),
            ],
        )
        .unwrap();
    let state = LocalState::from_root(dir.path().join("state"));

    let outcome = import_at(&path, &state, &["claude:id-deepseek".into()]).unwrap();
    assert_eq!(outcome.imported_count, 1);
    assert_eq!(outcome.usage_script_imported_count, 1);
    let profile = profiles(&state).remove(0);
    assert!(matches!(
        profile.usage_query,
        Some(asb_core::contracts::UsageQuery::Script { .. })
    ));
}

/// A Codex row with the Zhipu coding-plan template completes and persists the
/// synthesized native query through the strict store's own validation.
#[test]
fn codex_zhipu_token_plan_row_imports_with_a_native_usage_query() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_db(dir.path());
    let connection = Connection::open(&path).unwrap();
    insert_real_codex_with_meta(
        &connection,
        "id-zhipu",
        serde_json::json!({
            "usage_script": {
                "enabled": true,
                "language": "javascript",
                "code": "",
                "templateType": "token_plan",
                "codingPlanProvider": "zhipu"
            }
        })
        .to_string(),
    );
    let state = LocalState::from_root(dir.path().join("state"));

    let outcome = import_at(&path, &state, &["codex:id-zhipu".into()]).unwrap();
    assert_eq!(outcome.imported_count, 1);
    assert_eq!(outcome.usage_script_imported_count, 1);
    let records = state.configuration().list_codex_providers().unwrap();
    assert!(matches!(
        records[0].usage_query,
        Some(asb_core::contracts::UsageQuery::Script { .. })
    ));
}
