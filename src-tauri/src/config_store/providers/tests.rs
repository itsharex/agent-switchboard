use crate::config_store::{ConfigStore, ProfileStoreError, StoreOperationError};
use asb_core::contracts::{AppKind, ProviderDraft, RouteMode, UpstreamProtocol};
use std::collections::BTreeMap;

fn draft(name: &str) -> ProviderDraft {
    ProviderDraft {
        app: AppKind::Claude,
        route_mode: RouteMode::Custom,
        name: name.to_string(),
        base_url: Some("https://claude-relay.example".to_string()),
        api_key: "fixture-key".to_string(),
        upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
        responses_options: None,
        max_output_tokens: None.into(),
        model: None,
        model_options: None,
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

fn store() -> (tempfile::TempDir, ConfigStore) {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    (directory, store)
}

#[test]
fn claude_provider_updates_and_reorders_with_file_revisions() {
    let (_directory, store) = store();
    let first = store.create_provider(draft("first")).unwrap();
    let second = store.create_provider(draft("second")).unwrap();
    let mut changed = draft("renamed");
    changed.notes = Some("local note".to_string());
    let saved = store
        .update_provider(&first.profile.id, changed, &first.file_hash)
        .unwrap();
    assert_ne!(saved.file_hash, first.file_hash);
    let revisions = BTreeMap::from([
        (saved.profile.id.clone(), saved.file_hash.clone()),
        (second.profile.id.clone(), second.file_hash.clone()),
    ]);
    let ordered = store
        .reorder_providers(
            AppKind::Claude,
            &[second.profile.id.clone(), saved.profile.id.clone()],
            &revisions,
        )
        .unwrap();
    assert_eq!(ordered[0].profile.id, second.profile.id);
    assert!(store
        .delete_provider(&saved.profile.id, &first.file_hash)
        .is_err());
    let saved_after_reorder = ordered
        .iter()
        .find(|record| record.profile.id == saved.profile.id)
        .unwrap();
    store
        .delete_provider(&saved.profile.id, &saved_after_reorder.file_hash)
        .unwrap();
    assert_eq!(store.list_providers().unwrap().len(), 1);
}

#[test]
fn generic_provider_mutations_reject_codex_third_party_before_any_file_is_written() {
    let (_directory, store) = store();
    let mut third_party = draft("third-party Codex shape");
    third_party.app = AppKind::Codex;
    third_party.parameters = asb_core::ownership::default_provider_parameters(AppKind::Codex);
    assert!(store.create_provider(third_party).is_err());
    assert!(store.list_codex_providers().unwrap().is_empty());
}

fn official_codex_draft() -> ProviderDraft {
    ProviderDraft {
        app: AppKind::Codex,
        route_mode: RouteMode::Official,
        name: "Codex 官方登录".to_string(),
        base_url: None,
        api_key: String::new(),
        upstream_protocol: None,
        responses_options: None,
        max_output_tokens: None.into(),
        model: None,
        model_options: None,
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Codex),
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: Some(30),
    }
}

#[test]
fn codex_official_login_record_round_trips_through_the_generic_store() {
    let (_directory, store) = store();
    let record = store.create_provider(official_codex_draft()).unwrap();
    assert_eq!(record.profile.app, AppKind::Codex);
    assert_eq!(record.profile.route_mode, RouteMode::Official);

    // Only the official record lives in the generic store; the strict Codex
    // store stays empty and its directory holds exactly this one file.
    assert!(store.list_codex_providers().unwrap().is_empty());
    let listed = store.list_providers().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].profile.id, record.profile.id);
    assert_eq!(
        listed[0].profile.official_quota_refresh_interval_minutes,
        Some(30)
    );
    assert_eq!(
        store.find_provider(&record.profile.id).unwrap().name,
        "Codex 官方登录"
    );
    assert_eq!(
        std::fs::read_dir(store.providers_dir(AppKind::Codex).join("official"))
            .unwrap()
            .count(),
        1
    );

    // A second official entry for the same client is rejected.
    assert!(store.create_provider(official_codex_draft()).is_err());

    let mut renamed = official_codex_draft();
    renamed.name = "重命名官方登录".to_string();
    let saved = store
        .update_provider(&record.profile.id, renamed, &record.file_hash)
        .unwrap();
    assert_eq!(saved.profile.name, "重命名官方登录");
    assert_ne!(saved.file_hash, record.file_hash);

    store
        .delete_provider(&record.profile.id, &saved.file_hash)
        .unwrap();
    assert!(store.list_providers().unwrap().is_empty());
}

#[test]
fn codex_official_record_needs_an_official_route_mode() {
    let (_directory, store) = store();
    let mut third_party = official_codex_draft();
    third_party.route_mode = RouteMode::Custom;
    third_party.base_url = Some("https://relay.example/v1".to_string());
    third_party.api_key = "fixture-key".to_string();
    third_party.upstream_protocol = Some(UpstreamProtocol::Responses);
    assert_eq!(
        store.create_provider(third_party).unwrap_err(),
        StoreOperationError::Invalid("Codex 第三方供应商必须使用专用档案格式".to_string())
    );
}

#[test]
fn malformed_claude_file_is_rejected_without_rewriting_it() {
    let (_directory, store) = store();
    let record = store.create_provider(draft("relay")).unwrap();
    let path = store
        .providers_dir(AppKind::Claude)
        .join(format!("{}.json", record.profile.id));
    let malformed = r#"{"id":"broken"}"#;
    std::fs::write(&path, malformed).unwrap();
    assert_eq!(store.list_providers(), Err(ProfileStoreError::Unsupported));
    assert_eq!(std::fs::read_to_string(path).unwrap(), malformed);
}

/// The store contract behind the UI's reset banner: a legacy or unusable
/// layout surfaces as the typed store error from every operation family,
/// never as a flattened validation message.
#[test]
fn store_level_failures_stay_typed_through_every_operation_family() {
    let (directory, store) = store();
    std::fs::create_dir_all(store.legacy_store_path().parent().unwrap()).unwrap();
    std::fs::write(store.legacy_store_path(), b"{}").unwrap();
    let unsupported = StoreOperationError::Store(ProfileStoreError::Unsupported);
    assert_eq!(
        store.create_provider(draft("relay")),
        Err(unsupported.clone())
    );
    assert_eq!(
        store.import_provider(draft("relay")),
        Err(unsupported.clone())
    );
    assert_eq!(
        store.find_provider("00000000-0000-0000-0000-000000000001"),
        Err(unsupported.clone())
    );
    assert_eq!(
        store.save_client_settings(
            AppKind::Claude,
            asb_core::ownership::default_client_settings(AppKind::Claude),
            "stale",
        ),
        Err(unsupported.clone())
    );
    assert_eq!(
        store.record_config_write(asb_core::contracts::ConfigWriteRecord {
            app: AppKind::Claude,
            profile_id: None,
            profile_name: None,
            content_hash: "a".repeat(64),
            backup_id: "backup".to_string(),
            at: "2026-09-11T12:00:00+08:00".to_string(),
            operation: asb_core::contracts::WriteOperation::Restore,
        }),
        Err(unsupported)
    );
    let _ = directory;
}
