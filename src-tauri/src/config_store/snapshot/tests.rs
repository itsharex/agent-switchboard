use super::activation::{activate_staged, Activation};
use super::decode::{decode_cloud_backup_snapshot, UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT};
use super::*;
use crate::config_store::ConfigStore;
use asb_core::contracts::{
    AppKind, ConfigValue, ProviderDraft, RouteMode, SettingValue, UpstreamProtocol,
};
use std::fs;
use std::path::Path;

#[path = "tests/responses_upgrade.rs"]
mod responses_upgrade;

fn draft(app: AppKind, name: &str) -> ProviderDraft {
    ProviderDraft {
        parameters: asb_core::ownership::default_provider_parameters(app),
        app,
        route_mode: RouteMode::Custom,
        name: name.to_string(),
        model: None,
        base_url: Some("https://relay.example".to_string()),
        api_key: "test-api-key".to_string(),
        upstream_protocol: Some(match app {
            AppKind::Codex => UpstreamProtocol::Responses,
            AppKind::Claude => UpstreamProtocol::AnthropicMessages,
        }),
        responses_options: (Some(match app {
            AppKind::Codex => UpstreamProtocol::Responses,
            AppKind::Claude => UpstreamProtocol::AnthropicMessages,
        }) == Some(asb_core::contracts::UpstreamProtocol::Responses))
        .then_some(asb_core::contracts::ResponsesOptions {
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

fn official_draft(app: AppKind, name: &str) -> ProviderDraft {
    ProviderDraft {
        parameters: asb_core::ownership::default_provider_parameters(app),
        app,
        route_mode: RouteMode::Official,
        name: name.to_string(),
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

fn live_snapshot() -> (tempfile::TempDir, ConfigStore, ConfigurationSnapshot) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let store = ConfigStore::new(directory.path().join("state"));
    store
        .create_provider(draft(AppKind::Codex, "网关"))
        .expect("provider");
    let snapshot = read_configuration_snapshot(&store).expect("snapshot");
    (directory, store, snapshot)
}

#[test]
fn snapshot_round_trips_through_enable() {
    let (directory, store, mut snapshot) = live_snapshot();
    snapshot.client_settings.insert(AppKind::Codex, {
        let mut settings = snapshot.client_settings[&AppKind::Codex].clone();
        settings.settings.insert(
            "approval_policy".into(),
            SettingValue::Explicit {
                value: ConfigValue::Str("on-request".into()),
            },
        );
        settings
    });

    enable_snapshot(&store, &snapshot).expect("enable");
    let read_back = read_configuration_snapshot(&store).expect("read back");
    assert_eq!(read_back, snapshot);
    assert_eq!(read_back.provider_count(), 1);
    assert!(!directory
        .path()
        .join("state")
        .join("settings.json")
        .exists());
}

#[test]
fn previous_cloud_snapshot_distributes_shared_values_to_each_provider() {
    let (_directory, store, _) = live_snapshot();
    store
        .create_provider(draft(AppKind::Codex, "second"))
        .unwrap();
    store
        .create_provider(official_draft(AppKind::Claude, "official"))
        .unwrap();
    let mut snapshot = read_configuration_snapshot(&store).unwrap();
    for file in snapshot.providers.get_mut(&AppKind::Codex).unwrap() {
        file.parameters.settings.insert(
            "model_reasoning_effort".into(),
            SettingValue::Explicit {
                value: ConfigValue::Str("high".into()),
            },
        );
    }
    let decoded =
        decode_cloud_backup_snapshot(&super::previous::snapshot_bytes(&snapshot)).unwrap();
    assert!(decoded.migrated);
    assert_eq!(decoded.snapshot, snapshot);
    let json = serde_json::to_value(decoded.snapshot).unwrap();
    assert_eq!(json["schemaVersion"], 5);
    assert!(json.get("common").is_none());
    assert!(json["clientSettings"].is_object());
}

#[test]
fn current_cloud_snapshot_requires_provider_parameters_and_rejects_old_fields() {
    let (_directory, _store, snapshot) = live_snapshot();
    let bytes = serde_json::to_vec(&snapshot).unwrap();
    assert!(!decode_cloud_backup_snapshot(&bytes).unwrap().migrated);
    let mut value = serde_json::to_value(&snapshot).unwrap();
    value["providers"]["codex"][0]
        .as_object_mut()
        .unwrap()
        .remove("parameters");
    assert_eq!(
        decode_cloud_backup_snapshot(&serde_json::to_vec(&value).unwrap()).unwrap_err(),
        UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT
    );
}

#[test]
fn snapshot_without_responses_capabilities_cannot_replace_local_data() {
    let (_directory, store, snapshot) = live_snapshot();
    let mut value = serde_json::to_value(&snapshot).unwrap();
    value["providers"]["codex"][0]
        .as_object_mut()
        .unwrap()
        .remove("responsesOptions");
    assert!(decode_cloud_backup_snapshot(&serde_json::to_vec(&value).unwrap()).is_err());
    let invalid: ConfigurationSnapshot = serde_json::from_value(value).unwrap();
    assert!(enable_snapshot(&store, &invalid).is_err());
    assert_eq!(read_configuration_snapshot(&store).unwrap(), snapshot);
}

#[test]
fn cloud_snapshot_rejects_older_unversioned_and_malformed_previous_shapes() {
    let (_directory, _store, snapshot) = live_snapshot();
    for version in [Some(1), Some(2), None] {
        let mut value = serde_json::to_value(&snapshot).unwrap();
        match version {
            Some(version) => {
                value["schemaVersion"] = serde_json::json!(version);
            }
            None => {
                value.as_object_mut().unwrap().remove("schemaVersion");
            }
        }
        assert_eq!(
            decode_cloud_backup_snapshot(&serde_json::to_vec(&value).unwrap()).unwrap_err(),
            "云端备份配置快照版本不受支持"
        );
    }
    let mut previous: serde_json::Value =
        serde_json::from_slice(&super::previous::snapshot_bytes(&snapshot)).unwrap();
    previous["common"]["codex"]["settings"]
        .as_object_mut()
        .unwrap()
        .remove("model_reasoning_effort");
    assert!(decode_cloud_backup_snapshot(&serde_json::to_vec(&previous).unwrap()).is_err());
    let mut previous: serde_json::Value =
        serde_json::from_slice(&super::previous::snapshot_bytes(&snapshot)).unwrap();
    previous["providers"]["codex"][0]["authScheme"] = serde_json::json!("bearer");
    assert!(decode_cloud_backup_snapshot(&serde_json::to_vec(&previous).unwrap()).is_err());
}

#[test]
fn an_invalid_snapshot_is_rejected_without_touching_the_live_layout() {
    let (_directory, store, mut snapshot) = live_snapshot();
    snapshot
        .providers
        .get_mut(&AppKind::Codex)
        .expect("codex group")[0]
        .id = "not-a-uuid".into();

    assert!(enable_snapshot(&store, &snapshot).is_err());
    let read_back = read_configuration_snapshot(&store).expect("live layout intact");
    assert_eq!(read_back.provider_count(), 1);
    assert!(read_back.providers[&AppKind::Codex][0].id != "not-a-uuid");
}

#[test]
fn history_positions_must_stay_ordered() {
    let (_directory, store, mut snapshot) = live_snapshot();
    let mut file = snapshot.providers[&AppKind::Codex][0].clone();
    file.id = uuid::Uuid::new_v4().to_string();
    file.name = "第二网关".into();
    file.position = 50; // lower than the first provider's 100
    snapshot
        .providers
        .get_mut(&AppKind::Codex)
        .expect("codex group")
        .push(file);
    assert!(enable_snapshot(&store, &snapshot).is_err());
}

#[test]
fn duplicate_uuid_across_clients_is_rejected_before_any_snapshot_write() {
    let (_directory, store, mut snapshot) = live_snapshot();
    let mut claude = snapshot.providers[&AppKind::Codex][0].clone();
    claude.position = 100;
    snapshot
        .providers
        .get_mut(&AppKind::Claude)
        .expect("claude group")
        .push(claude);

    let error = enable_snapshot(&store, &snapshot).expect_err("duplicate must fail");
    assert!(error.contains("重复"));
    assert_eq!(
        read_configuration_snapshot(&store)
            .unwrap()
            .provider_count(),
        1
    );
}

#[test]
fn failed_swap_restores_the_original_live_directory() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state_root = directory.path().join("state");
    let live = state_root.join("configuration");
    let staged = state_root.join("staging").join("configuration");
    fs::create_dir_all(&live).unwrap();
    fs::create_dir_all(&staged).unwrap();
    fs::write(live.join("marker"), "old").unwrap();
    fs::write(staged.join("marker"), "new").unwrap();

    let mut calls = 0;
    let mut rename = |from: &Path, to: &Path| {
        calls += 1;
        if calls == 2 {
            return Err(std::io::Error::other("injected staged activation failure"));
        }
        fs::rename(from, to)
    };
    let result = activate_staged(
        &live,
        &staged,
        &state_root.join("retired-original"),
        &mut rename,
    );

    assert!(matches!(result, Activation::Restored(_)));
    assert_eq!(fs::read_to_string(live.join("marker")).unwrap(), "old");
}

#[test]
fn failed_rollback_reports_the_preserved_recovery_paths() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state_root = directory.path().join("state");
    let live = state_root.join("configuration");
    let staged = state_root.join("staging").join("configuration");
    fs::create_dir_all(&live).unwrap();
    fs::create_dir_all(&staged).unwrap();
    fs::write(live.join("marker"), "old").unwrap();
    fs::write(staged.join("marker"), "new").unwrap();

    let mut calls = 0;
    let mut rename = |from: &Path, to: &Path| {
        calls += 1;
        if calls == 2 || calls == 3 {
            return Err(std::io::Error::other("injected swap failure"));
        }
        fs::rename(from, to)
    };
    let result = activate_staged(
        &live,
        &staged,
        &state_root.join("retired-original"),
        &mut rename,
    );

    let Activation::RecoveryRequired(error) = result else {
        panic!("rollback failure must remain observable")
    };
    assert!(error.contains("原配置保留在"));
    let retired = fs::read_dir(&state_root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("retired-"))
        })
        .expect("retired original remains");
    assert_eq!(fs::read_to_string(retired.join("marker")).unwrap(), "old");
    assert_eq!(fs::read_to_string(staged.join("marker")).unwrap(), "new");
}

#[test]
fn explicit_schema_three_transport_conversion_never_weakens_current_schema() {
    let (_directory, _store, snapshot) = live_snapshot();
    let mut old: serde_json::Value =
        serde_json::from_slice(&super::previous::snapshot_bytes(&snapshot)).unwrap();
    old["providers"]["codex"][0]["responsesOptions"]["supportsWebsockets"] =
        serde_json::json!(true);
    let converted = decode_cloud_backup_snapshot(&serde_json::to_vec(&old).unwrap()).unwrap();
    assert!(converted.migrated);
    assert_eq!(converted.snapshot, snapshot);
    assert_eq!(converted.snapshot.schema_version, 5);
    let mut current = serde_json::to_value(&snapshot).unwrap();
    current["providers"]["codex"][0]["responsesOptions"]["supportsWebsockets"] =
        serde_json::json!(false);
    assert!(decode_cloud_backup_snapshot(&serde_json::to_vec(&current).unwrap()).is_err());
    old["providers"]["codex"][0]["responsesOptions"]["supportsWebsockets"] =
        serde_json::json!("true");
    assert!(decode_cloud_backup_snapshot(&serde_json::to_vec(&old).unwrap()).is_err());
}
