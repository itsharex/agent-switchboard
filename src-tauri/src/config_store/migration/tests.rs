use super::*;
use crate::config_store::snapshot::{previous, read_configuration_snapshot, ConfigurationSnapshot};
use crate::config_store::{content_revision, PendingProfileSave, ProfileStoreError};
use asb_core::contracts::{
    AppKind, ConfigValue, ProviderDraft, RouteMode, SettingValue, UpstreamProtocol,
};
use asb_core::ownership::{default_client_settings, default_provider_parameters};
use serde_json::{json, Value};

pub(super) fn draft(app: AppKind, name: &str) -> ProviderDraft {
    ProviderDraft {
        app,
        name: name.into(),
        route_mode: RouteMode::Custom,
        parameters: default_provider_parameters(app),
        api_key: "isolated-dummy-key".into(),
        base_url: Some("https://example.com/v1".into()),
        upstream_protocol: Some(UpstreamProtocol::native_for(app)),
        responses_options: (Some(UpstreamProtocol::native_for(app))
            == Some(asb_core::contracts::UpstreamProtocol::Responses))
        .then_some(asb_core::contracts::ResponsesOptions {
            request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
        }),
        max_output_tokens: None.into(),
        model: Some("example-model".into()),
        model_options: None,
        notes: Some("retained notes".into()),
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

fn previous_store() -> (tempfile::TempDir, ConfigStore, ConfigurationSnapshot) {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    for app in [AppKind::Codex, AppKind::Claude] {
        for name in ["first", "second"] {
            store.create_provider(draft(app, name)).unwrap();
        }
    }
    let mut snapshot = read_configuration_snapshot(&store).unwrap();
    for app in [AppKind::Codex, AppKind::Claude] {
        let key = if app == AppKind::Codex {
            "model_reasoning_effort"
        } else {
            "effortLevel"
        };
        for file in snapshot.providers.get_mut(&app).unwrap() {
            file.parameters.settings.insert(
                key.into(),
                SettingValue::Explicit {
                    value: ConfigValue::Str("high".into()),
                },
            );
        }
    }
    snapshot
        .client_settings
        .get_mut(&AppKind::Codex)
        .unwrap()
        .settings
        .insert(
            "approval_policy".into(),
            SettingValue::Explicit {
                value: ConfigValue::Str("on-request".into()),
            },
        );
    let old: Value = serde_json::from_slice(&previous::snapshot_bytes(&snapshot)).unwrap();
    fs::remove_dir(store.configuration_dir().join("client-settings")).unwrap();
    for app in [AppKind::Codex, AppKind::Claude] {
        let name = app.dir_name();
        write_json_atomic(
            &store
                .configuration_dir()
                .join("common")
                .join(format!("{name}.json")),
            &serde_json::to_string_pretty(&old["common"][name]).unwrap(),
        )
        .unwrap();
        for file in old["providers"][name].as_array().unwrap() {
            write_json_atomic(
                &store
                    .providers_dir(app)
                    .join(format!("{}.json", file["id"].as_str().unwrap())),
                &serde_json::to_string_pretty(file).unwrap(),
            )
            .unwrap();
        }
    }
    (directory, store, snapshot)
}

#[test]
fn upgrades_each_provider_and_client_settings_before_runtime_exposes_them() {
    let (_directory, store, expected) = previous_store();
    assert_eq!(store.list_providers(), Err(ProfileStoreError::Unsupported));
    assert!(store.upgrade_if_needed().unwrap());
    assert_eq!(read_configuration_snapshot(&store).unwrap(), expected);
    assert!(!store.configuration_dir().join("common").exists());
    assert!(store.client_settings_path(AppKind::Codex).is_file());
    assert!(!journal_path(&store).exists());
    assert!(!store.upgrade_if_needed().unwrap());
}

#[test]
fn previous_responses_provider_without_a_mode_becomes_standard_once() {
    let (_directory, store, expected) = previous_store();
    let file = &expected.providers[&AppKind::Codex][0];
    let path = store
        .providers_dir(AppKind::Codex)
        .join(format!("{}.json", file.id));
    let mut value: Value = layout::read_json(&path).unwrap().unwrap();
    value.as_object_mut().unwrap().remove("responsesOptions");
    fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();

    store.upgrade_if_needed().unwrap();

    let migrated = read_configuration_snapshot(&store).unwrap();
    assert_eq!(
        migrated.providers[&AppKind::Codex][0]
            .responses_options
            .as_ref()
            .unwrap()
            .request_mode,
        asb_core::contracts::ResponsesRequestMode::Standard
    );
}
#[test]
fn only_startup_converts_old_data_and_runtime_readers_never_rewrite_it() {
    let (_directory, store, expected) = previous_store();
    let state = crate::local_state::LocalState::from_root(store.state_root.clone());
    assert_eq!(
        state.configuration().list_providers(),
        Err(ProfileStoreError::Unsupported)
    );
    state.initialize_schemas().unwrap();
    assert_eq!(
        read_configuration_snapshot(&state.configuration()).unwrap(),
        expected
    );
    let file = &expected.providers[&AppKind::Codex][0];
    let path = store
        .providers_dir(AppKind::Codex)
        .join(format!("{}.json", file.id));
    let mut value: Value = layout::read_json(&path).unwrap().unwrap();
    value.as_object_mut().unwrap().remove("parameters");
    let old = serde_json::to_string_pretty(&value).unwrap();
    fs::write(&path, &old).unwrap();
    assert_eq!(
        state.configuration().list_providers(),
        Err(ProfileStoreError::Unsupported)
    );
    assert_eq!(fs::read_to_string(path).unwrap(), old);
}

#[test]
fn a_current_provider_missing_parameters_is_rejected_even_before_preferences_were_saved() {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    let record = store
        .create_provider(draft(AppKind::Codex, "new provider"))
        .unwrap();
    assert!(!store.client_settings_path(AppKind::Codex).exists());
    assert!(store.configuration_dir().join("client-settings").is_dir());
    let path = store
        .providers_dir(AppKind::Codex)
        .join(format!("{}.json", record.profile.id));
    let mut value: Value = layout::read_json(&path).unwrap().unwrap();
    value.as_object_mut().unwrap().remove("parameters");
    fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
    let before = layout::fingerprint(&store.configuration_dir()).unwrap();
    assert!(store.upgrade_if_needed().is_err());
    assert_eq!(store.list_providers(), Err(ProfileStoreError::Unsupported));
    assert_eq!(
        layout::fingerprint(&store.configuration_dir()).unwrap(),
        before
    );
}

#[test]
fn current_codex_profiles_gain_provider_owned_subagent_defaults_once() {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    let record = store
        .create_provider(draft(AppKind::Codex, "current provider"))
        .unwrap();
    let expected = read_configuration_snapshot(&store).unwrap();
    let path = store
        .providers_dir(AppKind::Codex)
        .join(format!("{}.json", record.profile.id));
    let mut value: Value = layout::read_json(&path).unwrap().unwrap();
    let settings = value["parameters"]["settings"].as_object_mut().unwrap();
    settings.remove("agents.default_subagent_model");
    settings.remove("agents.default_subagent_reasoning_effort");
    write_json_atomic(&path, &serde_json::to_string_pretty(&value).unwrap()).unwrap();

    assert!(store.upgrade_if_needed().unwrap());
    assert_eq!(read_configuration_snapshot(&store).unwrap(), expected);
    assert!(!store.upgrade_if_needed().unwrap());
}

#[test]
fn partial_subagent_parameter_upgrade_preserves_existing_data() {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    let record = store
        .create_provider(draft(AppKind::Codex, "partial provider"))
        .unwrap();
    let path = store
        .providers_dir(AppKind::Codex)
        .join(format!("{}.json", record.profile.id));
    let mut value: Value = layout::read_json(&path).unwrap().unwrap();
    value["parameters"]["settings"]
        .as_object_mut()
        .unwrap()
        .remove("agents.default_subagent_model");
    let original = serde_json::to_string_pretty(&value).unwrap();
    write_json_atomic(&path, &original).unwrap();

    assert!(store
        .upgrade_if_needed()
        .unwrap_err()
        .contains("必须同时存在"));
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}

#[test]
fn previous_absent_shared_file_preserves_automatic_semantics() {
    let (_directory, store, mut expected) = previous_store();
    fs::remove_dir_all(store.configuration_dir().join("common")).unwrap();
    assert_eq!(store.list_providers(), Err(ProfileStoreError::Unsupported));
    for app in [AppKind::Codex, AppKind::Claude] {
        for file in expected.providers.get_mut(&app).unwrap() {
            file.parameters = default_provider_parameters(app);
        }
        expected
            .client_settings
            .insert(app, default_client_settings(app));
    }
    store.upgrade_if_needed().unwrap();
    assert_eq!(read_configuration_snapshot(&store).unwrap(), expected);
}

#[test]
fn malformed_or_partial_values_leave_the_entire_previous_layout_untouched() {
    for invalid in [
        json!({"settings": {}}),
        json!({"settings": {"unknown": {"mode": "automatic"}}}),
    ] {
        let (_directory, store, _) = previous_store();
        write_json_atomic(
            &store.configuration_dir().join("common/codex.json"),
            &serde_json::to_string(&invalid).unwrap(),
        )
        .unwrap();
        let before = layout::fingerprint(&store.configuration_dir()).unwrap();
        assert!(store.upgrade_if_needed().is_err());
        assert_eq!(
            layout::fingerprint(&store.configuration_dir()).unwrap(),
            before
        );
        assert!(!journal_path(&store).exists());
    }
}

#[test]
fn mixed_provider_contracts_and_retired_authentication_fields_are_rejected() {
    for field in ["parameters", "authScheme"] {
        let (_directory, store, expected) = previous_store();
        let file = &expected.providers[&AppKind::Codex][0];
        let path = store
            .providers_dir(AppKind::Codex)
            .join(format!("{}.json", file.id));
        let mut value: Value = layout::read_json(&path).unwrap().unwrap();
        value[field] = if field == "parameters" {
            serde_json::to_value(&file.parameters).unwrap()
        } else {
            json!("bearer")
        };
        write_json_atomic(&path, &serde_json::to_string(&value).unwrap()).unwrap();
        let before = layout::fingerprint(&store.configuration_dir()).unwrap();
        assert!(store.upgrade_if_needed().is_err());
        assert_eq!(
            layout::fingerprint(&store.configuration_dir()).unwrap(),
            before
        );
    }
}

#[test]
fn unknown_material_is_never_dropped_by_configuration_upgrade() {
    let (_directory, store, _) = previous_store();
    fs::write(
        store.configuration_dir().join("personal-notes.txt"),
        "keep me",
    )
    .unwrap();
    let before = layout::fingerprint(&store.configuration_dir()).unwrap();
    assert!(store.upgrade_if_needed().unwrap_err().contains("未知材料"));
    assert_eq!(
        layout::fingerprint(&store.configuration_dir()).unwrap(),
        before
    );
}

fn stage_interrupted(store: &ConfigStore, snapshot: &ConfigurationSnapshot) -> (PathBuf, PathBuf) {
    let journal = recovery::UpgradeJournal::new();
    let (staging, retired) = journal.paths(store).unwrap();
    stage_and_verify(&staging.join("configuration"), snapshot).unwrap();
    write_json_atomic(
        &journal_path(store),
        &serde_json::to_string(&journal).unwrap(),
    )
    .unwrap();
    (staging, retired)
}

#[test]
fn interruption_between_directory_renames_restores_then_retries_the_whole_upgrade() {
    let (_directory, store, expected) = previous_store();
    let (staging, retired) = stage_interrupted(&store, &expected);
    fs::rename(store.configuration_dir(), &retired).unwrap();
    assert_eq!(store.list_providers(), Err(ProfileStoreError::Unsupported));
    assert!(store.upgrade_if_needed().unwrap());
    assert_eq!(read_configuration_snapshot(&store).unwrap(), expected);
    assert!(!staging.exists());
    assert!(!retired.exists());
}

#[test]
fn interruption_after_activation_verifies_the_new_layout_before_cleaning_up() {
    let (_directory, store, expected) = previous_store();
    let (staging, retired) = stage_interrupted(&store, &expected);
    fs::rename(store.configuration_dir(), &retired).unwrap();
    fs::rename(staging.join("configuration"), store.configuration_dir()).unwrap();
    assert!(!store.upgrade_if_needed().unwrap());
    assert_eq!(read_configuration_snapshot(&store).unwrap(), expected);
    assert!(retired.is_dir());
    assert!(!journal_path(&store).exists());
}

#[test]
fn interruption_after_cancelled_staging_cleanup_retries_the_unchanged_previous_layout() {
    let (_directory, store, expected) = previous_store();
    let (staging, _retired) = stage_interrupted(&store, &expected);
    fs::remove_dir_all(&staging).unwrap();
    assert!(store.upgrade_if_needed().unwrap());
    assert_eq!(read_configuration_snapshot(&store).unwrap(), expected);
    assert!(!journal_path(&store).exists());
}

#[test]
fn invalid_activated_layout_keeps_the_preimage_and_recovery_journal() {
    let (_directory, store, expected) = previous_store();
    let (staging, retired) = stage_interrupted(&store, &expected);
    fs::rename(store.configuration_dir(), &retired).unwrap();
    fs::rename(staging.join("configuration"), store.configuration_dir()).unwrap();
    fs::write(store.client_settings_path(AppKind::Codex), "{}").unwrap();
    assert!(store.upgrade_if_needed().is_err());
    assert!(retired.exists());
    assert!(journal_path(&store).exists());
}

#[test]
fn pending_save_identity_distinguishes_unwritten_and_confirmed_profile_changes() {
    for changed in [false, true] {
        let (_directory, store, mut expected) = previous_store();
        let file = &mut expected.providers.get_mut(&AppKind::Codex).unwrap()[0];
        let path = store
            .providers_dir(AppKind::Codex)
            .join(format!("{}.json", file.id));
        let previous_file_hash = content_revision(&fs::read(&path).unwrap());
        let pending = PendingProfileSave {
            profile_id: file.id.clone(),
            app: AppKind::Codex,
            previous_file_hash,
        };
        write_json_atomic(
            &store.profile_save_journal_path(),
            &serde_json::to_string(&pending).unwrap(),
        )
        .unwrap();
        if changed {
            let mut value: Value = layout::read_json(&path).unwrap().unwrap();
            value["model"] = json!("confirmed-change");
            file.model = Some("confirmed-change".into());
            write_json_atomic(&path, &serde_json::to_string(&value).unwrap()).unwrap();
        }
        store.upgrade_if_needed().unwrap();
        assert_eq!(read_configuration_snapshot(&store).unwrap(), expected);
        assert_eq!(
            store.pending_profile_save().unwrap(),
            changed.then_some(pending)
        );
    }
}

#[test]
fn malformed_recovery_paths_cannot_escape_the_state_directory() {
    let (_directory, store, _) = previous_store();
    write_json_atomic(&journal_path(&store), r#"{"id":"../outside"}"#).unwrap();
    let before = layout::fingerprint(&store.configuration_dir()).unwrap();
    assert!(store.upgrade_if_needed().is_err());
    assert_eq!(
        layout::fingerprint(&store.configuration_dir()).unwrap(),
        before
    );
}

#[test]
fn empty_provider_groups_with_explicit_parameters_preserve_previous_file_bytes() {
    for (app, key, value) in [
        (
            AppKind::Codex,
            "features.fast_mode",
            ConfigValue::Bool(false),
        ),
        (
            AppKind::Codex,
            "model_reasoning_summary",
            ConfigValue::Str("auto".into()),
        ),
        (
            AppKind::Claude,
            "alwaysThinkingEnabled",
            ConfigValue::Bool(false),
        ),
        (
            AppKind::Claude,
            "effortLevel",
            ConfigValue::Str("high".into()),
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(directory.path().join("state"));
        let mut settings = previous::previous_defaults(app);
        settings
            .settings
            .insert(key.into(), SettingValue::Explicit { value });
        let path = store
            .configuration_dir()
            .join("common")
            .join(format!("{}.json", app.dir_name()));
        let original = format!("{}\n", serde_json::to_string_pretty(&settings).unwrap());
        write_json_atomic(&path, &original).unwrap();
        let before = layout::fingerprint(&store.configuration_dir()).unwrap();

        let error = store
            .upgrade_if_needed()
            .expect_err("explicit intent needs a provider owner");

        assert!(error.contains(app.label()), "{error}");
        assert!(error.contains(key), "{error}");
        assert_eq!(fs::read(&path).unwrap(), original.as_bytes());
        assert_eq!(
            layout::fingerprint(&store.configuration_dir()).unwrap(),
            before
        );
        assert!(!journal_path(&store).exists());
        assert!(!store.configuration_dir().join("client-settings").exists());
    }
}

#[test]
fn empty_provider_groups_with_automatic_parameters_still_upgrade_locally() {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    for app in [AppKind::Codex, AppKind::Claude] {
        write_json_atomic(
            &store
                .configuration_dir()
                .join("common")
                .join(format!("{}.json", app.dir_name())),
            &serde_json::to_string_pretty(&previous::previous_defaults(app)).unwrap(),
        )
        .unwrap();
    }

    assert!(store.upgrade_if_needed().unwrap());

    let snapshot = read_configuration_snapshot(&store).unwrap();
    assert_eq!(snapshot.provider_count(), 0);
    for app in [AppKind::Codex, AppKind::Claude] {
        assert_eq!(snapshot.client_settings[&app], default_client_settings(app));
    }
    assert!(!store.configuration_dir().join("common").exists());
}
