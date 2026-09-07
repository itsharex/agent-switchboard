use super::crypto::{decrypt, encrypt, encrypt_cleartext};
use super::remote::{restore_with_request, test_connection_with_request, upload_with_request};
use super::*;
use crate::config_store::snapshot::{
    decode_cloud_backup_snapshot, read_configuration_snapshot, ConfigurationSnapshot,
};
use asb_core::contracts::{
    AppKind, ClaudeModelSettings, CodexModelSettings, CommonSettingValue, ConfigValue,
    ConfigWriteRecord, ModelOptions, ProviderDraft, RouteMode, UpstreamProtocol, UsageQuery,
    WriteOperation,
};
use std::cell::RefCell;

fn connection_settings() -> CloudBackupSettings {
    CloudBackupSettings {
        project_url: "https://example.supabase.co".to_string(),
        publishable_key: "sb_publishable_example".to_string(),
        email: "backup@example.com".to_string(),
    }
}

fn custom_draft(app: AppKind, name: &str) -> ProviderDraft {
    ProviderDraft {
        app,
        route_mode: RouteMode::Custom,
        name: name.to_string(),
        model: None,
        base_url: Some("https://relay.example/v1".to_string()),
        api_key: format!("{name}-key"),
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

fn migrated_payload(
    mut cleartext: Vec<u8>,
    backup_password: &str,
) -> (ConfigurationSnapshot, EncryptedBackup) {
    let decoded = decode_cloud_backup_snapshot(&cleartext).expect("legacy snapshot migrates");
    assert!(decoded.migrated);
    let payload = encrypt_cleartext(&mut cleartext, backup_password).expect("encrypt legacy");
    (decoded.snapshot, payload)
}

fn auth_response() -> String {
    r#"{"access_token":"session-value","user":{"id":"c9d2eeb1-425e-4f9d-8ff4-bd27e52103fb"}}"#
        .to_string()
}

#[test]
fn upload_and_restore_round_trip_the_complete_configuration_snapshot() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = LocalState::from_root(directory.path().join("source-state"));
    source
        .set_cloud_backup_settings(&connection_settings())
        .expect("source connection settings");
    let source_store = source.configuration();
    let codex = source_store
        .create_provider(ProviderDraft {
            app: AppKind::Codex,
            route_mode: RouteMode::Custom,
            name: "Codex relay".to_string(),
            model: Some("gpt-5.6-codex".to_string()),
            base_url: Some("https://codex-relay.example/v1".to_string()),
            api_key: "fixture-codex-value".to_string(),
            upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
            max_output_tokens: Some(8_192).into(),
            model_options: Some(ModelOptions::Codex(CodexModelSettings {
                context_window: Some(272_000),
            })),
            notes: Some("primary coding route".to_string()),
            website_url: Some("https://codex-relay.example".to_string()),
            usage_query: Some(UsageQuery::Declarative {
                url: "{{baseUrl}}/usage".to_string(),
                remaining_path: Some("data/remaining".to_string()),
                used_path: Some("data/used".to_string()),
                total_path: Some("data/total".to_string()),
                unit: Some("credits".to_string()),
                refresh_interval_minutes: 15,
            }),
            official_quota_refresh_interval_minutes: None,
        })
        .expect("codex provider");
    source_store
        .create_provider(ProviderDraft {
            app: AppKind::Codex,
            route_mode: RouteMode::Official,
            name: "Codex official".to_string(),
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
        })
        .expect("codex official provider");
    let claude = source_store
        .create_provider(ProviderDraft {
            app: AppKind::Claude,
            route_mode: RouteMode::Custom,
            name: "Claude relay".to_string(),
            model: Some("claude-opus-4-1".to_string()),
            base_url: Some("https://claude-relay.example".to_string()),
            api_key: "fixture-claude-value".to_string(),
            upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
            max_output_tokens: None.into(),
            model_options: Some(ModelOptions::Claude(ClaudeModelSettings {
                primary_one_m: true,
                haiku_model: Some("claude-haiku-4".to_string()),
                sonnet_model: Some("claude-sonnet-4-6".to_string()),
                sonnet_one_m: true,
                opus_model: Some("claude-opus-4-1".to_string()),
                opus_one_m: true,
                available_models: Some(vec![
                    "claude-haiku-4".to_string(),
                    "claude-opus-4-1".to_string(),
                ]),
            })),
            notes: Some("primary analysis route".to_string()),
            website_url: Some("https://claude-relay.example/docs".to_string()),
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        })
        .expect("claude provider");
    let codex_common = source_store
        .get_common_settings(AppKind::Codex)
        .expect("codex common settings");
    let mut codex_settings = codex_common.settings;
    codex_settings.settings.insert(
        "model_reasoning_effort".to_string(),
        CommonSettingValue::Explicit {
            value: ConfigValue::Str("xhigh".to_string()),
        },
    );
    source_store
        .save_common_settings(AppKind::Codex, codex_settings, &codex_common.settings_hash)
        .expect("save codex common settings");
    for (profile, at) in [
        (&codex.profile, "2026-09-04T12:00:00Z"),
        (&claude.profile, "2026-09-04T12:01:00Z"),
    ] {
        source_store
            .record_config_write(ConfigWriteRecord {
                app: profile.app,
                profile_id: Some(profile.id.clone()),
                profile_name: Some(profile.name.clone()),
                content_hash: "a".repeat(64),
                backup_id: format!("backup-{}", profile.id),
                at: at.to_string(),
                operation: WriteOperation::Projection,
            })
            .expect("switch history");
    }
    let snapshot = read_configuration_snapshot(&source_store).expect("source snapshot");
    assert_eq!(snapshot.providers[&AppKind::Codex].len(), 2);
    assert_eq!(snapshot.providers[&AppKind::Claude].len(), 1);
    assert_eq!(snapshot.history[&AppKind::Codex].len(), 1);
    assert_eq!(snapshot.history[&AppKind::Claude].len(), 1);

    let requests = RefCell::new(Vec::new());
    let uploaded = upload_with_request(
            &source,
            "project-auth-password",
            "cloud-backup-password",
            &|method, url, headers, body| {
                let index = requests.borrow().len();
                requests.borrow_mut().push((
                    method.to_string(),
                    url.to_string(),
                    headers.to_string(),
                    body.to_vec(),
                ));
                match index {
                    0 => Ok((
                        200,
                        r#"{"access_token":"session-value","user":{"id":"c9d2eeb1-425e-4f9d-8ff4-bd27e52103fb"}}"#
                            .to_string(),
                    )),
                    1 => Ok((201, String::new())),
                    _ => panic!("unexpected upload request"),
                }
            },
        )
        .expect("upload");
    assert_eq!(uploaded.profile_count, 3);
    assert!(!uploaded.migrated);
    let requests = requests.into_inner();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].0, "POST");
    assert!(requests[1]
        .1
        .ends_with("/rest/v1/agent_switchboard_cloud_backups?on_conflict=user_id"));
    let upload_body: serde_json::Value =
        serde_json::from_slice(&requests[1].3).expect("encrypted upload JSON");
    assert_eq!(
        upload_body["user_id"],
        "c9d2eeb1-425e-4f9d-8ff4-bd27e52103fb"
    );
    assert!(upload_body.get("userId").is_none());
    let upload_text = String::from_utf8(requests[1].3.clone()).expect("upload text");
    assert!(!upload_text.contains("fixture-codex-value"));
    assert!(!upload_text.contains("fixture-claude-value"));
    let payload: EncryptedBackup =
        serde_json::from_value(upload_body["payload"].clone()).expect("encrypted payload");

    let target = LocalState::from_root(directory.path().join("target-state"));
    target
        .set_cloud_backup_settings(&connection_settings())
        .expect("target connection settings");
    target
        .configuration()
        .create_provider(ProviderDraft {
            app: AppKind::Codex,
            route_mode: RouteMode::Custom,
            name: "stale route".to_string(),
            model: None,
            base_url: Some("https://stale.example".to_string()),
            api_key: "stale-value".to_string(),
            upstream_protocol: Some(UpstreamProtocol::Responses),
            max_output_tokens: None.into(),
            model_options: None,
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        })
        .expect("stale target provider");
    let restore_response = serde_json::json!([{
        "payload": payload,
        "updated_at": uploaded.updated_at,
    }])
    .to_string();
    let restore_requests = RefCell::new(0usize);
    let restored = restore_with_request(
            &target,
            "project-auth-password",
            "cloud-backup-password",
            &|_, _, _, _| {
                let index = *restore_requests.borrow();
                *restore_requests.borrow_mut() += 1;
                match index {
                    0 => Ok((
                        200,
                        r#"{"access_token":"session-value","user":{"id":"c9d2eeb1-425e-4f9d-8ff4-bd27e52103fb"}}"#
                            .to_string(),
                    )),
                    1 => Ok((200, restore_response.clone())),
                    _ => panic!("unexpected restore request"),
                }
            },
        )
        .expect("restore");
    assert_eq!(restored.profile_count, 3);
    assert!(!restored.migrated);
    assert_eq!(*restore_requests.borrow(), 2);
    assert_eq!(
        read_configuration_snapshot(&target.configuration()).expect("restored snapshot"),
        snapshot
    );
}

#[test]
fn restore_migrates_v2_cloud_snapshot_and_rewrites_remote_payload() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = LocalState::from_root(directory.path().join("v2-source"));
    source
        .configuration()
        .create_provider(custom_draft(AppKind::Codex, "v2-codex"))
        .expect("v2 codex");
    source
        .configuration()
        .create_provider(custom_draft(AppKind::Claude, "v2-claude"))
        .expect("v2 claude");
    let source_snapshot =
        read_configuration_snapshot(&source.configuration()).expect("source snapshot");
    let (expected_snapshot, payload) =
        migrated_payload(v2_snapshot_bytes(&source_snapshot), "backup-password");
    assert_eq!(
        expected_snapshot.providers[&AppKind::Codex][0].upstream_protocol,
        Some(UpstreamProtocol::Responses)
    );
    assert_eq!(
        expected_snapshot.providers[&AppKind::Claude][0].upstream_protocol,
        Some(UpstreamProtocol::AnthropicMessages)
    );

    let target = LocalState::from_root(directory.path().join("restore-target"));
    target
        .set_cloud_backup_settings(&connection_settings())
        .expect("target connection settings");
    target
        .configuration()
        .create_provider(custom_draft(AppKind::Codex, "stale-route"))
        .expect("stale target provider");
    let restore_response = serde_json::json!([{
        "payload": payload,
        "updated_at": "2026-09-06T12:00:00.000Z",
    }])
    .to_string();
    let requests = RefCell::new(Vec::new());

    let restored = restore_with_request(
        &target,
        "project-auth-password",
        "backup-password",
        &|method, url, headers, body| {
            let index = requests.borrow().len();
            requests.borrow_mut().push((
                method.to_string(),
                url.to_string(),
                headers.to_string(),
                body.to_vec(),
            ));
            match index {
                0 => Ok((200, auth_response())),
                1 => Ok((200, restore_response.clone())),
                2 => Ok((201, String::new())),
                _ => panic!("unexpected restore request"),
            }
        },
    )
    .expect("v2 restore");

    assert!(restored.migrated);
    assert_eq!(restored.profile_count, 2);
    assert_eq!(
        read_configuration_snapshot(&target.configuration()).expect("restored snapshot"),
        expected_snapshot
    );
    let requests = requests.into_inner();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[2].0, "POST");
    assert!(requests[2]
        .1
        .ends_with("/rest/v1/agent_switchboard_cloud_backups?on_conflict=user_id"));
    let remote_body: serde_json::Value =
        serde_json::from_slice(&requests[2].3).expect("migration upload JSON");
    assert_eq!(
        remote_body["user_id"],
        "c9d2eeb1-425e-4f9d-8ff4-bd27e52103fb"
    );
    assert_eq!(
        remote_body["updated_at"].as_str(),
        Some(restored.updated_at.as_str())
    );
    let upgraded_payload: EncryptedBackup =
        serde_json::from_value(remote_body["payload"].clone()).expect("migration payload");
    let mut upgraded_cleartext =
        decrypt(&upgraded_payload, "backup-password").expect("migration payload decrypts");
    let upgraded: ConfigurationSnapshot =
        serde_json::from_slice(&upgraded_cleartext).expect("current snapshot JSON");
    upgraded_cleartext.fill(0);
    assert_eq!(upgraded, expected_snapshot);
    assert_eq!(
        upgraded.schema_version,
        crate::config_store::snapshot::CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION
    );
    let upload_text = String::from_utf8(requests[2].3.clone()).expect("migration upload text");
    assert!(!upload_text.contains("v2-codex-key"));
    assert!(!upload_text.contains("v2-claude-key"));
}

#[test]
fn legacy_restore_does_not_replace_local_data_when_remote_upgrade_fails() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = LocalState::from_root(directory.path().join("v2-source"));
    source
        .configuration()
        .create_provider(custom_draft(AppKind::Codex, "v2-codex"))
        .expect("v2 codex");
    let source_snapshot =
        read_configuration_snapshot(&source.configuration()).expect("source snapshot");
    let (_, payload) = migrated_payload(v2_snapshot_bytes(&source_snapshot), "backup-password");

    let target = LocalState::from_root(directory.path().join("restore-target"));
    target
        .set_cloud_backup_settings(&connection_settings())
        .expect("target connection settings");
    target
        .configuration()
        .create_provider(custom_draft(AppKind::Codex, "stale-route"))
        .expect("stale target provider");
    let before = read_configuration_snapshot(&target.configuration()).expect("stale snapshot");
    let restore_response = serde_json::json!([{
        "payload": payload,
        "updated_at": "2026-09-06T12:00:00.000Z",
    }])
    .to_string();
    let request_count = RefCell::new(0usize);

    let error = restore_with_request(
        &target,
        "project-auth-password",
        "backup-password",
        &|_, _, _, _| {
            let index = *request_count.borrow();
            *request_count.borrow_mut() += 1;
            match index {
                0 => Ok((200, auth_response())),
                1 => Ok((200, restore_response.clone())),
                2 => Ok((503, String::new())),
                _ => panic!("unexpected restore request"),
            }
        },
    )
    .expect_err("remote migration failure must stop before local restore");

    assert_eq!(
        error,
        "云端备份格式升级失败，本机配置未改变：Supabase 云端备份请求失败（HTTP 503）"
    );
    assert_eq!(*request_count.borrow(), 3);
    assert_eq!(
        read_configuration_snapshot(&target.configuration()).expect("local snapshot"),
        before
    );
}

#[test]
fn incorrect_backup_password_never_decrypts_the_snapshot() {
    let snapshot = ConfigurationSnapshot {
        schema_version: crate::config_store::snapshot::CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION,
        providers: Default::default(),
        common: Default::default(),
        history: Default::default(),
    };
    let encrypted = encrypt(&snapshot, "cloud-backup-password").expect("encrypt");

    assert_eq!(
        decrypt(&encrypted, "wrong-password").expect_err("wrong password"),
        "备份密码不正确或云端备份已损坏"
    );
}

#[test]
fn setup_sql_enforces_authenticated_row_ownership() {
    assert!(SETUP_SQL.contains("enable row level security"));
    assert!(SETUP_SQL.contains("to authenticated"));
    assert!(SETUP_SQL.contains("(select auth.uid()) = user_id"));
    assert!(!SETUP_SQL.contains("service_role"));
}

#[test]
fn backup_records_use_the_database_column_names_over_data_api() {
    let payload = EncryptedBackup {
        version: ENCRYPTION_VERSION,
        salt: "salt".to_string(),
        nonce: "nonce".to_string(),
        ciphertext: "ciphertext".to_string(),
    };
    let record = serde_json::to_value(UploadRecord {
        user_id: "c9d2eeb1-425e-4f9d-8ff4-bd27e52103fb",
        payload: &payload,
        updated_at: "2026-09-04T12:00:00.000Z",
    })
    .expect("upload record serializes");

    assert_eq!(record["user_id"], "c9d2eeb1-425e-4f9d-8ff4-bd27e52103fb");
    assert_eq!(record["updated_at"], "2026-09-04T12:00:00.000Z");
    assert!(record.get("userId").is_none());
    assert!(record.get("updatedAt").is_none());

    let remote: RemoteRecord = serde_json::from_value(serde_json::json!({
        "payload": payload,
        "updated_at": "2026-09-04T12:00:00.000Z",
    }))
    .expect("Supabase record deserializes");
    assert_eq!(remote.updated_at, "2026-09-04T12:00:00.000Z");
}

#[test]
fn connection_test_authenticates_and_reads_only_the_callers_backup_row() {
    let calls = RefCell::new(Vec::new());
    test_connection_with_request(&connection_settings(), "account-password", |method, url, headers, body| {
            let index = calls.borrow().len();
            calls.borrow_mut().push((
                method.to_string(),
                url.to_string(),
                headers.to_string(),
                body.to_vec(),
            ));
            match index {
                0 => Ok((
                    200,
                    r#"{"access_token":"session-value","user":{"id":"c9d2eeb1-425e-4f9d-8ff4-bd27e52103fb"}}"#
                        .to_string(),
                )),
                1 => Ok((200, "[]".to_string())),
                _ => panic!("unexpected request"),
            }
        })
        .expect("connection succeeds");

    let calls = calls.into_inner();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].0, "POST");
    assert_eq!(
        calls[0].1,
        "https://example.supabase.co/auth/v1/token?grant_type=password"
    );
    assert_eq!(
        calls[0].2,
        "apikey: sb_publishable_example\r\nContent-Type: application/json"
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&calls[0].3).expect("auth request JSON"),
        serde_json::json!({
            "email": "backup@example.com",
            "password": "account-password",
        })
    );
    assert_eq!(calls[1].0, "GET");
    assert_eq!(
            calls[1].1,
            "https://example.supabase.co/rest/v1/agent_switchboard_cloud_backups?select=user_id&user_id=eq.c9d2eeb1-425e-4f9d-8ff4-bd27e52103fb&limit=1"
        );
    assert!(calls[1].2.contains("Authorization: Bearer session-value"));
    assert!(calls[1].3.is_empty());
}

#[test]
fn connection_test_reports_a_missing_or_unavailable_backup_table() {
    let request_count = RefCell::new(0usize);
    let error = test_connection_with_request(
            &connection_settings(),
            "account-password",
            |_, _, _, _| {
                let index = *request_count.borrow();
                *request_count.borrow_mut() += 1;
                if index == 0 {
                    Ok((
                        200,
                        r#"{"access_token":"session-value","user":{"id":"c9d2eeb1-425e-4f9d-8ff4-bd27e52103fb"}}"#
                            .to_string(),
                    ))
                } else {
                    Ok((404, String::new()))
                }
            },
        )
        .expect_err("missing table is not a successful connection");

    assert_eq!(
        error,
        "云端备份表不可用，请确认已启用 Data API 并在 Supabase SQL Editor 执行初始化 SQL"
    );
    assert_eq!(*request_count.borrow(), 2);
}
