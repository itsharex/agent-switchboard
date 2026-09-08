mod round_trip;

use super::crypto::{decrypt, encrypt, encrypt_cleartext};
use super::remote::{restore_with_request, test_connection_with_request, upload_with_request};
use super::*;
use crate::config_store::snapshot::{
    decode_cloud_backup_snapshot, read_configuration_snapshot, ConfigurationSnapshot,
};
use asb_core::contracts::{AppKind, ProviderDraft, RouteMode, UpstreamProtocol};
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
        parameters: asb_core::ownership::default_provider_parameters(app),
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
fn empty_provider_groups_with_explicit_parameters_never_overwrite_cloud_or_local_data() {
    for (app, key, value) in [
        (
            AppKind::Codex,
            "features.fast_mode",
            serde_json::json!(false),
        ),
        (
            AppKind::Codex,
            "model_reasoning_summary",
            serde_json::json!("auto"),
        ),
        (
            AppKind::Claude,
            "alwaysThinkingEnabled",
            serde_json::json!(false),
        ),
        (AppKind::Claude, "effortLevel", serde_json::json!("high")),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let target = LocalState::from_root(directory.path().join("restore-target"));
        target
            .set_cloud_backup_settings(&connection_settings())
            .unwrap();
        target
            .configuration()
            .create_provider(custom_draft(app, "retained-provider"))
            .unwrap();
        let before = read_configuration_snapshot(&target.configuration()).unwrap();
        let empty = LocalState::from_root(directory.path().join("empty-source"));
        let snapshot = read_configuration_snapshot(&empty.configuration()).unwrap();
        let mut previous: serde_json::Value = serde_json::from_slice(
            &crate::config_store::snapshot::previous::snapshot_bytes(&snapshot),
        )
        .unwrap();
        previous["common"][app.dir_name()]["settings"][key] =
            serde_json::json!({"mode": "explicit", "value": value});
        let mut cleartext = serde_json::to_vec(&previous).unwrap();
        let payload = encrypt_cleartext(&mut cleartext, "backup-password").unwrap();
        let response = serde_json::json!([{
            "payload": payload, "updated_at": "2026-09-07T00:00:00Z",
        }])
        .to_string();
        let requests = RefCell::new(Vec::new());

        let error = restore_with_request(
            &target,
            "account-password",
            "backup-password",
            &|method, url, _, _| {
                let index = requests.borrow().len();
                requests
                    .borrow_mut()
                    .push((method.to_string(), url.to_string()));
                match index {
                    0 => Ok((200, auth_response())),
                    1 => Ok((200, response.clone())),
                    _ => panic!("orphaned parameter must stop before any cloud overwrite"),
                }
            },
        )
        .expect_err("unowned explicit parameters cannot be discarded");

        assert!(error.contains(app.label()), "{error}");
        assert!(error.contains(key), "{error}");
        let requests = requests.into_inner();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].0, "POST");
        assert!(requests[0]
            .1
            .ends_with("/auth/v1/token?grant_type=password"));
        assert_eq!(requests[1].0, "GET");
        assert_eq!(
            read_configuration_snapshot(&target.configuration()).unwrap(),
            before
        );
    }
}

#[test]
fn empty_provider_groups_with_automatic_parameters_still_decode_from_cloud() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("empty-source"));
    let snapshot = read_configuration_snapshot(&state.configuration()).unwrap();

    let decoded = decode_cloud_backup_snapshot(
        &crate::config_store::snapshot::previous::snapshot_bytes(&snapshot),
    )
    .unwrap();

    assert!(decoded.migrated);
    assert_eq!(decoded.snapshot, snapshot);
    assert_eq!(decoded.snapshot.provider_count(), 0);
}

#[test]
fn restore_migrates_v3_cloud_snapshot_and_rewrites_remote_payload() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = LocalState::from_root(directory.path().join("v3-source"));
    source
        .configuration()
        .create_provider(custom_draft(AppKind::Codex, "v3-codex"))
        .expect("v3 codex");
    source
        .configuration()
        .create_provider(custom_draft(AppKind::Claude, "v3-claude"))
        .expect("v3 claude");
    let source_snapshot =
        read_configuration_snapshot(&source.configuration()).expect("source snapshot");
    let (expected_snapshot, payload) = migrated_payload(
        crate::config_store::snapshot::previous::snapshot_bytes(&source_snapshot),
        "backup-password",
    );
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
    .expect("v3 restore");

    assert!(restored.migrated);
    assert_eq!(restored.profile_count, 2);
    assert_eq!(
        read_configuration_snapshot(&target.configuration()).expect("restored snapshot"),
        expected_snapshot
    );
    assert_upgraded_remote_payload(&expected_snapshot, &restored, requests.into_inner());
}

#[test]
fn legacy_restore_does_not_replace_local_data_when_remote_upgrade_fails() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = LocalState::from_root(directory.path().join("v3-source"));
    source
        .configuration()
        .create_provider(custom_draft(AppKind::Codex, "v3-codex"))
        .expect("v3 codex");
    let source_snapshot =
        read_configuration_snapshot(&source.configuration()).expect("source snapshot");
    let (_, payload) = migrated_payload(
        crate::config_store::snapshot::previous::snapshot_bytes(&source_snapshot),
        "backup-password",
    );

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
        client_settings: Default::default(),
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

fn assert_upgraded_remote_payload(
    expected_snapshot: &ConfigurationSnapshot,
    restored: &CloudBackupResult,
    requests: Vec<(String, String, String, Vec<u8>)>,
) {
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
    assert_eq!(&upgraded, expected_snapshot);
    assert_eq!(
        upgraded.schema_version,
        crate::config_store::snapshot::CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION
    );
    let upload_text = String::from_utf8(requests[2].3.clone()).expect("migration upload text");
    assert!(!upload_text.contains("v3-codex-key"));
    assert!(!upload_text.contains("v3-claude-key"));
}
