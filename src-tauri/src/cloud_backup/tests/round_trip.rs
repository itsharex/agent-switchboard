use super::*;
use asb_core::contracts::{
    ClaudeModelSettings, CodexModelSettings, ConfigValue, ConfigWriteRecord, ModelOptions,
    SettingValue, UsageQuery, WriteOperation,
};
use std::path::Path;

fn codex_draft() -> ProviderDraft {
    let mut draft = custom_draft(AppKind::Codex, "Codex relay");
    draft.model = Some("gpt-5.6-codex".into());
    draft.base_url = Some("https://codex-relay.example/v1".into());
    draft.api_key = "fixture-codex-value".into();
    draft.upstream_protocol = Some(UpstreamProtocol::AnthropicMessages);
    draft.responses_options = None;
    draft.max_output_tokens = Some(8_192).into();
    draft.model_options = Some(ModelOptions::Codex(CodexModelSettings {
        context_window: Some(272_000),
    }));
    draft.notes = Some("primary coding route".into());
    draft.website_url = Some("https://codex-relay.example".into());
    draft.usage_query = Some(UsageQuery::Declarative {
        url: "{{baseUrl}}/usage".into(),
        remaining_path: Some("data/remaining".into()),
        used_path: Some("data/used".into()),
        total_path: Some("data/total".into()),
        unit: Some("credits".into()),
        refresh_interval_minutes: 15,
    });
    draft.parameters.settings.insert(
        "model_reasoning_effort".into(),
        SettingValue::Explicit {
            value: ConfigValue::Str("xhigh".into()),
        },
    );
    draft
}

fn claude_draft() -> ProviderDraft {
    let mut draft = custom_draft(AppKind::Claude, "Claude relay");
    draft.model = Some("claude-opus-4-1".into());
    draft.base_url = Some("https://claude-relay.example".into());
    draft.api_key = "fixture-claude-value".into();
    draft.model_options = Some(ModelOptions::Claude(ClaudeModelSettings {
        primary_one_m: true,
        haiku_model: Some("claude-haiku-4".into()),
        sonnet_model: Some("claude-sonnet-4-6".into()),
        sonnet_one_m: true,
        opus_model: Some("claude-opus-4-1".into()),
        opus_one_m: true,
        available_models: Some(vec!["claude-haiku-4".into(), "claude-opus-4-1".into()]),
    }));
    draft.notes = Some("primary analysis route".into());
    draft.website_url = Some("https://claude-relay.example/docs".into());
    draft
}

fn source_configuration(root: &Path) -> (LocalState, ConfigurationSnapshot) {
    let source = LocalState::from_root(root.join("source-state"));
    source
        .set_cloud_backup_settings(&connection_settings())
        .unwrap();
    let store = source.configuration();
    let codex = store.create_provider(codex_draft()).unwrap();
    let claude = store.create_provider(claude_draft()).unwrap();
    let mut official = custom_draft(AppKind::Codex, "Codex official");
    official.route_mode = RouteMode::Official;
    official.base_url = None;
    official.api_key.clear();
    official.upstream_protocol = None;
    official.responses_options = None;
    store.create_provider(official).unwrap();
    let client = store.get_client_settings(AppKind::Codex).unwrap();
    let mut settings = client.settings;
    settings.settings.insert(
        "approval_policy".into(),
        SettingValue::Explicit {
            value: ConfigValue::Str("on-request".into()),
        },
    );
    store
        .save_client_settings(AppKind::Codex, settings, &client.settings_hash)
        .unwrap();
    for (profile, at) in [
        (&codex.profile, "2026-09-04T12:00:00Z"),
        (&claude.profile, "2026-09-04T12:01:00Z"),
    ] {
        store
            .record_config_write(ConfigWriteRecord {
                app: profile.app,
                profile_id: Some(profile.id.clone()),
                profile_name: Some(profile.name.clone()),
                content_hash: "a".repeat(64),
                backup_id: format!("backup-{}", profile.id),
                at: at.into(),
                operation: WriteOperation::Projection,
            })
            .unwrap();
    }
    let snapshot = read_configuration_snapshot(&store).unwrap();
    assert_eq!(snapshot.providers[&AppKind::Codex].len(), 2);
    assert_eq!(snapshot.providers[&AppKind::Claude].len(), 1);
    assert_eq!(snapshot.history[&AppKind::Codex].len(), 1);
    assert_eq!(snapshot.history[&AppKind::Claude].len(), 1);
    assert_ne!(
        snapshot.providers[&AppKind::Codex][0].parameters,
        snapshot.providers[&AppKind::Codex][1].parameters
    );
    (source, snapshot)
}

fn upload_payload(source: &LocalState) -> (CloudBackupResult, EncryptedBackup) {
    let requests = RefCell::new(Vec::new());
    let uploaded = upload_with_request(
        source,
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
                0 => Ok((200, auth_response())),
                1 => Ok((201, String::new())),
                _ => panic!("unexpected upload request"),
            }
        },
    )
    .unwrap();
    assert_eq!(uploaded.profile_count, 3);
    assert!(!uploaded.migrated);
    let requests = requests.into_inner();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].0, "POST");
    assert!(requests[1]
        .1
        .ends_with("/rest/v1/agent_switchboard_cloud_backups?on_conflict=user_id"));
    let body: serde_json::Value = serde_json::from_slice(&requests[1].3).unwrap();
    assert_eq!(body["user_id"], "c9d2eeb1-425e-4f9d-8ff4-bd27e52103fb");
    assert!(body.get("userId").is_none());
    let text = String::from_utf8(requests[1].3.clone()).unwrap();
    assert!(!text.contains("fixture-codex-value"));
    assert!(!text.contains("fixture-claude-value"));
    let payload = serde_json::from_value(body["payload"].clone()).unwrap();
    (uploaded, payload)
}

fn restore_payload(
    root: &Path,
    uploaded: CloudBackupResult,
    payload: EncryptedBackup,
) -> ConfigurationSnapshot {
    let target = LocalState::from_root(root.join("target-state"));
    target
        .set_cloud_backup_settings(&connection_settings())
        .unwrap();
    target
        .configuration()
        .create_provider(custom_draft(AppKind::Codex, "stale route"))
        .unwrap();
    let response =
        serde_json::json!([{"payload": payload, "updated_at": uploaded.updated_at}]).to_string();
    let requests = RefCell::new(0usize);
    let restored = restore_with_request(
        &target,
        "project-auth-password",
        "cloud-backup-password",
        &|_, _, _, _| {
            let index = *requests.borrow();
            *requests.borrow_mut() += 1;
            match index {
                0 => Ok((200, auth_response())),
                1 => Ok((200, response.clone())),
                _ => panic!("unexpected restore request"),
            }
        },
    )
    .unwrap();
    assert_eq!(restored.profile_count, 3);
    assert!(!restored.migrated);
    assert_eq!(*requests.borrow(), 2);
    read_configuration_snapshot(&target.configuration()).unwrap()
}

#[test]
fn upload_and_restore_round_trip_the_complete_configuration_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let (source, snapshot) = source_configuration(directory.path());
    let (uploaded, payload) = upload_payload(&source);
    assert_eq!(
        restore_payload(directory.path(), uploaded, payload),
        snapshot
    );
}
