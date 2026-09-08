mod invalid;

use crate::config_store::{ConfigStore, ProfileStoreError};
use asb_core::contracts::RouteMode;
use asb_core::contracts::{
    AppKind, ProviderDraft, ProviderFile, ProviderRecord, UpstreamProtocol, UsageQuery,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

fn codex_draft(name: &str) -> ProviderDraft {
    ProviderDraft {
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Codex),
        app: AppKind::Codex,
        route_mode: RouteMode::Custom,
        name: name.to_string(),
        model: Some("gpt-5.3-codex".to_string()),
        base_url: Some("https://gateway.example/v1".to_string()),
        api_key: "OPENAI_API_KEY".to_string(),
        upstream_protocol: Some(UpstreamProtocol::Responses),
        responses_options: Some(asb_core::contracts::ResponsesOptions {
            request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
        }),
        max_output_tokens: None.into(),
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

fn claude_draft(name: &str) -> ProviderDraft {
    ProviderDraft {
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
        app: AppKind::Claude,
        route_mode: RouteMode::Custom,
        name: name.to_string(),
        model: None,
        base_url: Some("https://claude-relay.example".to_string()),
        api_key: "test-api-key".to_string(),
        upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
        responses_options: None,
        max_output_tokens: None.into(),
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

fn official_draft(app: AppKind) -> ProviderDraft {
    ProviderDraft {
        parameters: asb_core::ownership::default_provider_parameters(app),
        app,
        route_mode: RouteMode::Official,
        name: match app {
            AppKind::Codex => "Codex 官方登录",
            AppKind::Claude => "Claude 官方登录",
        }
        .to_string(),
        model: None,
        base_url: None,
        api_key: String::new(),
        upstream_protocol: None,
        responses_options: None,
        max_output_tokens: None.into(),
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

fn store() -> (tempfile::TempDir, ConfigStore) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let store = ConfigStore::new(directory.path().join("state"));
    (directory, store)
}

fn provider_files(store: &ConfigStore, app: AppKind) -> Vec<PathBuf> {
    let entries = match fs::read_dir(store.providers_dir(app)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return vec![],
        Err(error) => panic!("provider dir: {error}"),
    };
    let mut paths: Vec<PathBuf> = entries
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();
    paths
}

/// Reads the position straight out of the named provider file.
fn stored_position(store: &ConfigStore, app: AppKind, id: &str) -> u64 {
    let path = store.providers_dir(app).join(format!("{id}.json"));
    let file: ProviderFile = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    file.position
}

fn file_for(store: &ConfigStore, app: AppKind, id: &str) -> PathBuf {
    store.providers_dir(app).join(format!("{id}.json"))
}

fn revisions(records: &[ProviderRecord], app: AppKind) -> BTreeMap<String, String> {
    records
        .iter()
        .filter(|record| record.profile.app == app)
        .map(|record| (record.profile.id.clone(), record.file_hash.clone()))
        .collect()
}

#[test]
fn create_writes_one_uuid_named_file_per_provider() {
    let (_dir, store) = store();
    let first = store.create_provider(codex_draft("网关 A")).expect("first");
    let second = store
        .create_provider(codex_draft("网关 B"))
        .expect("second");
    store
        .create_provider(claude_draft("Claude 中继"))
        .expect("claude");

    assert!(Uuid::parse_str(&first.profile.id).is_ok());
    let codex_files = provider_files(&store, AppKind::Codex);
    assert_eq!(codex_files.len(), 2);
    assert_eq!(
        stored_position(&store, AppKind::Codex, &first.profile.id),
        100
    );
    assert_eq!(
        stored_position(&store, AppKind::Codex, &second.profile.id),
        200
    );
    assert_eq!(provider_files(&store, AppKind::Claude).len(), 1);

    // The file carries no client field.
    let text = fs::read_to_string(file_for(&store, AppKind::Codex, &first.profile.id)).unwrap();
    assert!(!text.contains("\"app\""));
}

#[test]
fn update_preserves_position_and_rejects_external_changes() {
    let (_dir, store) = store();
    let created = store.create_provider(codex_draft("网关")).expect("create");
    let updated = store
        .update_provider(
            &created.profile.id,
            codex_draft("改名网关"),
            &created.file_hash,
        )
        .expect("update");
    assert_ne!(updated.file_hash, created.file_hash);
    assert_eq!(
        stored_position(&store, AppKind::Codex, &created.profile.id),
        100
    );
    assert_eq!(store.list_providers().unwrap()[0].profile.name, "改名网关");

    let error = store
        .update_provider(&created.profile.id, codex_draft("外部改过"), "stale-hash")
        .expect_err("stale hash must fail");
    assert!(error.contains("外部修改"));
    assert_eq!(store.list_providers().unwrap()[0].profile.name, "改名网关");
}

#[test]
fn delete_removes_only_that_file() {
    let (_dir, store) = store();
    let first = store.create_provider(codex_draft("网关 A")).expect("first");
    let second = store
        .create_provider(codex_draft("网关 B"))
        .expect("second");
    store
        .delete_provider(&first.profile.id, &first.file_hash)
        .expect("delete");

    let records = store.list_providers().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].profile.id, second.profile.id);
}

#[test]
fn reorder_rewrites_positions_for_that_client_only() {
    let (_dir, store) = store();
    let a = store.create_provider(codex_draft("网关 A")).expect("A");
    let claude = store
        .create_provider(claude_draft("Claude 中继"))
        .expect("claude");
    let b = store.create_provider(codex_draft("网关 B")).expect("B");
    let claude_hash_before =
        fs::read_to_string(file_for(&store, AppKind::Claude, &claude.profile.id)).unwrap();
    let before = store.list_providers().unwrap();

    let reordered = store
        .reorder_providers(
            AppKind::Codex,
            &[b.profile.id.clone(), a.profile.id.clone()],
            &revisions(&before, AppKind::Codex),
        )
        .expect("reorder");
    let codex_order: Vec<&str> = reordered
        .iter()
        .filter(|record| record.profile.app == AppKind::Codex)
        .map(|record| record.profile.name.as_str())
        .collect();
    assert_eq!(codex_order, vec!["网关 B", "网关 A"]);
    assert!(reordered
        .iter()
        .any(|record| record.profile.id == claude.profile.id));
    let claude_hash_after =
        fs::read_to_string(file_for(&store, AppKind::Claude, &claude.profile.id)).unwrap();
    assert_eq!(claude_hash_before, claude_hash_after);

    let error = store
        .reorder_providers(
            AppKind::Codex,
            &[a.profile.id.clone()],
            &revisions(&reordered, AppKind::Codex),
        )
        .expect_err("incomplete list must fail");
    assert!(error.contains("不得重复") || error.contains("一一对应"));
}

#[test]
fn delete_and_reorder_reject_stale_provider_revisions() {
    let (_dir, store) = store();
    let first = store.create_provider(codex_draft("网关 A")).unwrap();
    let second = store.create_provider(codex_draft("网关 B")).unwrap();

    let first_path = file_for(&store, AppKind::Codex, &first.profile.id);
    fs::write(
        &first_path,
        fs::read_to_string(&first_path)
            .unwrap()
            .replace("网关 A", "外部修改"),
    )
    .unwrap();
    let delete_error = store
        .delete_provider(&first.profile.id, &first.file_hash)
        .expect_err("external provider edit must block delete");
    assert!(delete_error.contains("外部修改"));
    assert!(first_path.exists());

    let stale = BTreeMap::from([
        (first.profile.id.clone(), first.file_hash.clone()),
        (second.profile.id.clone(), second.file_hash.clone()),
    ]);
    let reorder_error = store
        .reorder_providers(
            AppKind::Codex,
            &[second.profile.id.clone(), first.profile.id.clone()],
            &stale,
        )
        .expect_err("external provider edit must block reorder");
    assert!(reorder_error.contains("外部修改"));
    assert_eq!(
        stored_position(&store, AppKind::Codex, &first.profile.id),
        100
    );
    assert_eq!(
        stored_position(&store, AppKind::Codex, &second.profile.id),
        200
    );
}

#[test]
fn duplicate_ids_or_positions_in_provider_files_are_rejected() {
    let (_dir, store) = store();
    let first = store.create_provider(codex_draft("网关 A")).unwrap();
    let second = store.create_provider(codex_draft("网关 B")).unwrap();
    let second_path = file_for(&store, AppKind::Codex, &second.profile.id);
    let mut second_file: ProviderFile =
        serde_json::from_str(&fs::read_to_string(&second_path).unwrap()).unwrap();
    second_file.position = 100;
    fs::write(&second_path, serde_json::to_string(&second_file).unwrap()).unwrap();
    assert_eq!(store.list_providers(), Err(ProfileStoreError::Unsupported));

    fs::remove_file(&second_path).unwrap();
    let claude_path = file_for(&store, AppKind::Claude, &first.profile.id);
    fs::create_dir_all(claude_path.parent().unwrap()).unwrap();
    let mut duplicate: ProviderFile = serde_json::from_str(
        &fs::read_to_string(file_for(&store, AppKind::Codex, &first.profile.id)).unwrap(),
    )
    .unwrap();
    duplicate.position = 100;
    fs::write(&claude_path, serde_json::to_string(&duplicate).unwrap()).unwrap();
    assert_eq!(store.list_providers(), Err(ProfileStoreError::Unsupported));
}

#[test]
fn import_is_idempotent_and_enriches_a_routing_match_with_a_query() {
    let (_dir, store) = store();
    let first = store
        .import_provider(codex_draft("导入网关"))
        .expect("first");
    let second = store
        .import_provider(codex_draft("导入网关"))
        .expect("second");
    assert_eq!(first.profile.id, second.profile.id);
    assert_eq!(store.list_providers().unwrap().len(), 1);

    let mut with_query = codex_draft("导入网关");
    with_query.usage_query = Some(UsageQuery::Script {
        source: r#"({
                request() { return { url: "https://gateway.example/usage", method: "GET" }; },
                extract() { return { remaining: 1, unit: "USD" }; }
            })"#
        .to_string(),
        refresh_interval_minutes: 0,
    });
    assert!(!store.provider_exists(&with_query));
    assert!(store.provider_will_receive_usage_query(&with_query));
    let updated = store.import_provider(with_query.clone()).expect("enrich");
    assert_eq!(updated.profile.id, first.profile.id);
    assert_eq!(updated.profile.usage_query, with_query.usage_query);
    assert_eq!(store.list_providers().unwrap().len(), 1);
}

#[test]
fn import_routing_identity_includes_protocol_and_output_limit() {
    let (_dir, store) = store();
    let mut direct = codex_draft("同名中继");
    direct.upstream_protocol = Some(UpstreamProtocol::ChatCompletions);
    direct.responses_options = None;
    store.import_provider(direct.clone()).expect("direct route");

    let mut anthropic_limit_a = direct.clone();
    anthropic_limit_a.upstream_protocol = Some(UpstreamProtocol::AnthropicMessages);
    anthropic_limit_a.responses_options = None;
    anthropic_limit_a.max_output_tokens = Some(1_024).into();
    store
        .import_provider(anthropic_limit_a.clone())
        .expect("first explicit Anthropic limit");

    let mut anthropic_limit_b = anthropic_limit_a;
    anthropic_limit_b.max_output_tokens = Some(2_048).into();
    store
        .import_provider(anthropic_limit_b)
        .expect("different output limit is a different route");

    assert_eq!(store.list_providers().unwrap().len(), 3);
}

#[test]
fn official_import_is_idempotent_per_client_and_never_needs_a_credential() {
    let (_dir, store) = store();
    let codex = store
        .import_provider(official_draft(AppKind::Codex))
        .expect("official Codex route");
    let duplicate = store
        .import_provider(official_draft(AppKind::Codex))
        .expect("same official route");
    let claude = store
        .import_provider(official_draft(AppKind::Claude))
        .expect("official Claude route");

    assert_eq!(codex.profile.id, duplicate.profile.id);
    assert_eq!(codex.profile.route_mode, RouteMode::Official);
    assert!(codex.profile.api_key.is_empty());
    assert_eq!(claude.profile.route_mode, RouteMode::Official);
    assert_eq!(store.list_providers().unwrap().len(), 2);
}

#[test]
fn invalid_usage_scripts_never_reach_a_file() {
    let (_dir, store) = store();
    let mut draft = codex_draft("坏脚本");
    draft.usage_query = Some(UsageQuery::Script {
        source: "({ request() {} })".to_string(),
        refresh_interval_minutes: 0,
    });
    assert!(store.create_provider(draft).is_err());
    assert_eq!(provider_files(&store, AppKind::Codex).len(), 0);
}

#[test]
fn foreign_files_are_rejected_loudly() {
    let (_dir, store) = store();
    store.create_provider(codex_draft("网关")).expect("create");
    let path = provider_files(&store, AppKind::Codex)
        .into_iter()
        .next()
        .unwrap();
    fs::write(&path, "{\"id\":\"not-a-uuid\",\"name\":\"x\"}").unwrap();
    assert!(matches!(
        store.list_providers().expect_err("bad file must fail"),
        ProfileStoreError::Unsupported
    ));
}

#[test]
fn official_quota_interval_round_trips_and_absent_stays_manual() {
    let (_dir, store) = store();
    let mut draft = official_draft(AppKind::Codex);
    draft.official_quota_refresh_interval_minutes = Some(15);
    let record = store
        .create_provider(draft)
        .expect("official codex provider");
    assert_eq!(
        record.profile.official_quota_refresh_interval_minutes,
        Some(15)
    );

    let path = file_for(&store, AppKind::Codex, &record.profile.id);
    let text = fs::read_to_string(path).expect("stored file");
    assert!(text.contains("\"officialQuotaRefreshIntervalMinutes\": 15"));

    // A file saved before the interval existed stays valid and reads as
    // manual-only; there is no default fill or rewrite.
    let without_interval = text.replacen(",\n  \"officialQuotaRefreshIntervalMinutes\": 15", "", 1);
    assert_ne!(without_interval, text);
    let parsed: ProviderFile = serde_json::from_str(&without_interval).expect("old file parses");
    assert_eq!(parsed.official_quota_refresh_interval_minutes, None);
}
