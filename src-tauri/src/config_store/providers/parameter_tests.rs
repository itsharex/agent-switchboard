use crate::config_store::ConfigStore;
use asb_core::contracts::{
    AppKind, ConfigValue, ProviderDraft, RouteMode, SettingValue, UpstreamProtocol,
};
use asb_core::ownership::default_provider_parameters;

fn draft(name: &str) -> ProviderDraft {
    ProviderDraft {
        app: AppKind::Codex,
        route_mode: RouteMode::Custom,
        name: name.into(),
        parameters: default_provider_parameters(AppKind::Codex),
        base_url: Some("https://relay.example/v1".into()),
        api_key: "fixture-value".into(),
        upstream_protocol: Some(UpstreamProtocol::Responses),
        responses_options: Some(asb_core::contracts::ResponsesOptions {
            request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
        }),
        max_output_tokens: None.into(),
        model: None,
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

#[test]
fn parameter_edits_change_only_the_selected_provider_and_its_revision() {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    let first = store.create_provider(draft("first")).unwrap();
    let second = store.create_provider(draft("second")).unwrap();
    let client = store.get_client_settings(AppKind::Codex).unwrap();
    let mut updated = draft("first");
    updated.parameters.settings.insert(
        "model_reasoning_effort".into(),
        SettingValue::Explicit {
            value: ConfigValue::Str("high".into()),
        },
    );
    let saved = store
        .update_provider(&first.profile.id, updated, &first.file_hash)
        .unwrap();
    assert_ne!(saved.file_hash, first.file_hash);
    assert_eq!(
        store.find_provider_record(&second.profile.id).unwrap(),
        second
    );
    assert_eq!(store.get_client_settings(AppKind::Codex).unwrap(), client);
    let restored = store
        .update_provider(&first.profile.id, draft("first"), &saved.file_hash)
        .unwrap();
    assert_eq!(
        restored.profile.parameters,
        default_provider_parameters(AppKind::Codex)
    );
    assert_eq!(
        store.find_provider_record(&second.profile.id).unwrap(),
        second
    );
}

#[test]
fn provider_parameters_cannot_be_saved_to_client_preferences() {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    let client = store.get_client_settings(AppKind::Codex).unwrap();
    assert!(store
        .save_client_settings(
            AppKind::Codex,
            default_provider_parameters(AppKind::Codex),
            &client.settings_hash
        )
        .is_err());
    assert_eq!(store.get_client_settings(AppKind::Codex).unwrap(), client);
}

#[test]
fn imported_parameters_are_not_lost_when_an_existing_connection_can_gain_a_usage_query() {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    let original = store.import_provider(draft("imported")).unwrap();
    let mut changed = draft("imported");
    changed.parameters.settings.insert(
        "model_reasoning_effort".into(),
        SettingValue::Explicit {
            value: ConfigValue::Str("high".into()),
        },
    );
    changed.usage_query = Some(asb_core::contracts::UsageQuery::Declarative {
        url: "https://relay.example/usage".into(),
        remaining_path: Some("balance".into()),
        used_path: None,
        total_path: None,
        unit: None,
        refresh_interval_minutes: 0,
    });
    assert!(!store.provider_exists(&changed));
    assert!(!store.provider_will_receive_usage_query(&changed));
    let imported = store.import_provider(changed.clone()).unwrap();
    assert_ne!(imported.profile.id, original.profile.id);
    assert_eq!(imported.profile.parameters, changed.parameters);
    assert_eq!(
        store.find_provider_record(&original.profile.id).unwrap(),
        original
    );
}
