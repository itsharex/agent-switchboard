use super::activation::{activate_staged, Activation};
use super::legacy::{decode_cloud_backup_snapshot, UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT};
use super::*;
use crate::config_store::ConfigStore;
use asb_core::contracts::{
    AppKind, CommonSettingValue, ConfigValue, ProviderDraft, RouteMode, UpstreamProtocol,
};
use std::fs;
use std::path::Path;

fn draft(app: AppKind, name: &str) -> ProviderDraft {
    ProviderDraft {
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
        app,
        route_mode: RouteMode::Official,
        name: name.to_string(),
        model: None,
        base_url: None,
        api_key: String::new(),
        upstream_protocol: None,
        max_output_tokens: None.into(),
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

fn legacy_snapshot_bytes(snapshot: &ConfigurationSnapshot) -> Vec<u8> {
    let mut value = serde_json::to_value(snapshot).expect("snapshot JSON");
    let root = value.as_object_mut().expect("snapshot object");
    root.remove("schemaVersion");
    let providers = root
        .get_mut("providers")
        .and_then(serde_json::Value::as_object_mut)
        .expect("provider groups");
    for files in providers.values_mut() {
        for file in files.as_array_mut().expect("provider files") {
            let provider = file.as_object_mut().expect("provider object");
            provider.remove("upstreamProtocol");
            provider.remove("authScheme");
            provider.remove("maxOutputTokens");
        }
    }
    serde_json::to_vec(&value).expect("legacy JSON")
}

fn v2_snapshot_bytes(snapshot: &ConfigurationSnapshot) -> Vec<u8> {
    let mut value = serde_json::to_value(snapshot).expect("snapshot JSON");
    let root = value.as_object_mut().expect("snapshot object");
    root.insert("schemaVersion".to_string(), serde_json::Value::from(2));
    let providers = root
        .get_mut("providers")
        .and_then(serde_json::Value::as_object_mut)
        .expect("provider groups");
    for files in providers.values_mut() {
        for file in files.as_array_mut().expect("provider files") {
            let provider = file.as_object_mut().expect("provider object");
            let authentication = match provider
                .get("upstreamProtocol")
                .and_then(serde_json::Value::as_str)
            {
                Some("anthropicMessages") => "xApiKey",
                Some(_) => "bearer",
                None => "none",
            };
            provider.insert(
                "authScheme".to_string(),
                serde_json::Value::String(authentication.to_string()),
            );
        }
    }
    serde_json::to_vec(&value).expect("v2 JSON")
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
    snapshot.common.insert(AppKind::Codex, {
        let mut settings = snapshot.common[&AppKind::Codex].clone();
        settings.settings.insert(
            "model_reasoning_effort".into(),
            CommonSettingValue::Explicit {
                value: ConfigValue::Str("high".into()),
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
fn legacy_cloud_snapshot_migrates_the_historical_routes_deterministically() {
    let (_directory, store, _) = live_snapshot();
    store
        .create_provider(official_draft(AppKind::Codex, "Codex official"))
        .expect("codex official");
    store
        .create_provider(draft(AppKind::Claude, "Claude relay"))
        .expect("claude custom");
    store
        .create_provider(official_draft(AppKind::Claude, "Claude official"))
        .expect("claude official");
    let snapshot = read_configuration_snapshot(&store).expect("current snapshot");

    let decoded = decode_cloud_backup_snapshot(&legacy_snapshot_bytes(&snapshot))
        .expect("historical snapshot migrates");

    assert!(decoded.migrated);
    assert_eq!(decoded.snapshot, snapshot);
    let codex_custom = &decoded.snapshot.providers[&AppKind::Codex][0];
    assert_eq!(
        codex_custom.upstream_protocol,
        Some(UpstreamProtocol::Responses)
    );
    assert_eq!(codex_custom.max_output_tokens.value(), None);
    let codex_official = &decoded.snapshot.providers[&AppKind::Codex][1];
    assert_eq!(codex_official.upstream_protocol, None);
    let claude_custom = &decoded.snapshot.providers[&AppKind::Claude][0];
    assert_eq!(
        claude_custom.upstream_protocol,
        Some(UpstreamProtocol::AnthropicMessages)
    );
    assert_eq!(claude_custom.max_output_tokens.value(), None);
    let claude_official = &decoded.snapshot.providers[&AppKind::Claude][1];
    assert_eq!(claude_official.upstream_protocol, None);
}

#[test]
fn v2_cloud_snapshot_drops_the_obsolete_authentication_field() {
    let (_directory, _store, snapshot) = live_snapshot();
    let decoded =
        decode_cloud_backup_snapshot(&v2_snapshot_bytes(&snapshot)).expect("v2 snapshot migrates");

    assert!(decoded.migrated);
    assert_eq!(decoded.snapshot, snapshot);
    assert!(serde_json::to_string(&decoded.snapshot)
        .expect("current snapshot")
        .contains("\"schemaVersion\":3"));
}

#[test]
fn unversioned_current_cloud_snapshot_is_rewritten_without_losing_protocol_fields() {
    let (_directory, _store, mut snapshot) = live_snapshot();
    let provider = &mut snapshot.providers.get_mut(&AppKind::Codex).unwrap()[0];
    provider.upstream_protocol = Some(UpstreamProtocol::AnthropicMessages);
    provider.max_output_tokens = Some(8_192).into();
    validate_snapshot(&snapshot).expect("current protocol route");
    let mut value = serde_json::to_value(&snapshot).expect("snapshot JSON");
    value
        .as_object_mut()
        .expect("snapshot object")
        .remove("schemaVersion");

    let decoded = decode_cloud_backup_snapshot(
        &serde_json::to_vec(&value).expect("unversioned current JSON"),
    )
    .expect("current snapshot migrates");

    assert!(decoded.migrated);
    assert_eq!(decoded.snapshot, snapshot);
    assert_eq!(
        decoded.snapshot.providers[&AppKind::Codex][0]
            .max_output_tokens
            .value(),
        Some(8_192)
    );
}

#[test]
fn cloud_snapshot_rejects_an_unsupported_version_or_unknown_legacy_shape() {
    let (_directory, _store, mut snapshot) = live_snapshot();
    snapshot.schema_version = 1;
    let wrong_version = serde_json::to_vec(&snapshot).expect("wrong-version JSON");
    assert_eq!(
        decode_cloud_backup_snapshot(&wrong_version).expect_err("version must fail"),
        "云端备份配置快照版本不受支持"
    );

    let current = read_configuration_snapshot(&_store).expect("current snapshot");
    let mut legacy: serde_json::Value =
        serde_json::from_slice(&legacy_snapshot_bytes(&current)).expect("legacy JSON");
    legacy
        .as_object_mut()
        .expect("legacy object")
        .insert("unexpected".to_string(), serde_json::Value::Bool(true));
    assert_eq!(
        decode_cloud_backup_snapshot(&serde_json::to_vec(&legacy).expect("unknown JSON"))
            .expect_err("unknown legacy field must fail"),
        UNSUPPORTED_CLOUD_BACKUP_SNAPSHOT
    );
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
    let result = activate_staged(&live, &staged, &state_root, &mut rename);

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
    let result = activate_staged(&live, &staged, &state_root, &mut rename);

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
