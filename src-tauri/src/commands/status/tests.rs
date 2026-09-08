use super::overview::{runtime_overview_for, RuntimeBuildMode, RuntimeTransport};
use super::report::{active_profile_id, classify_match_status};
use asb_core::contracts::{
    AppKind, ConfigWriteRecord, MatchStatus, ProviderProfile, WriteOperation,
};
use std::path::Path;

fn profile() -> ProviderProfile {
    ProviderProfile {
        id: "p1".to_string(),
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Codex),
        app: AppKind::Codex,
        route_mode: asb_core::RouteMode::Custom,
        name: "当前档案".to_string(),
        model: Some("gpt-5.4".to_string()),
        base_url: Some("https://gateway.example/v1".to_string()),
        api_key: "test-api-key".into(),
        upstream_protocol: Some(asb_core::UpstreamProtocol::Responses),
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

fn log(
    content_hash: &str,
    profile_id: Option<&str>,
    profile_name: Option<&str>,
) -> ConfigWriteRecord {
    ConfigWriteRecord {
        app: AppKind::Codex,
        profile_id: profile_id.map(str::to_string),
        profile_name: profile_name.map(str::to_string),
        content_hash: content_hash.to_string(),
        backup_id: "b1".to_string(),
        at: "2026-08-26T08:00:00Z".to_string(),
        operation: WriteOperation::Projection,
    }
}

#[test]
fn match_status_never_activates_a_restored_backup_or_an_edited_profile() {
    let current = profile();
    let mut restore_record = log("same", None, None);
    restore_record.operation = WriteOperation::Restore;
    let restored = classify_match_status(Some(&restore_record), "same", Some(&current));
    assert!(matches!(restored, MatchStatus::RestoredBackup { .. }));

    let stale_profile =
        classify_match_status(Some(&log("same", Some("p1"), Some("旧档案"))), "same", None);
    assert_eq!(
        stale_profile,
        MatchStatus::ProfileChanged {
            profile_name: "旧档案".to_string(),
        }
    );

    let matching = classify_match_status(
        Some(&log("old", Some("p1"), Some("旧档案"))),
        "current",
        Some(&current),
    );
    assert_eq!(
        matching,
        MatchStatus::MatchesProfile {
            profile_id: "p1".to_string(),
            profile_name: "当前档案".to_string(),
        }
    );
}

#[test]
fn active_identity_is_independent_of_config_match_and_restores() {
    for app in [AppKind::Codex, AppKind::Claude] {
        let mut official = profile();
        official.app = app;
        official.parameters = asb_core::ownership::default_provider_parameters(app);
        official.route_mode = asb_core::RouteMode::Official;
        official.base_url = None;
        official.api_key.clear();
        official.upstream_protocol = None;
        official.responses_options = None;
        let text = match app {
            AppKind::Codex => "model = \"different-model\"\nmodel_reasoning_effort = \"high\"\n",
            AppKind::Claude => r#"{"model":"different-model","effortLevel":"high"}"#,
        };
        let mut restored = log("same", None, None);
        restored.operation = WriteOperation::Restore;
        assert_eq!(
            active_profile_id(&[official], None, app, text, Some(&restored)).unwrap(),
            Some("p1".into())
        );
    }
}

#[test]
fn retired_custom_identity_never_activates_a_profile() {
    let profiles = [profile()];
    let text = "model_provider = \"agent_switchboard\"\n";
    assert_eq!(
        active_profile_id(
            &profiles,
            None,
            AppKind::Codex,
            text,
            Some(&log("old", Some("p1"), None))
        )
        .unwrap(),
        None
    );
}

#[test]
fn runtime_overview_contains_only_application_runtime_facts() {
    let overview = runtime_overview_for(
        "0.1.5".to_string(),
        Path::new("C:/data/Agent Switchboard"),
        RuntimeTransport::WebDevelopment {
            host: "127.0.0.1".to_string(),
            port: 1422,
            health_status: 204,
        },
    );

    assert_eq!(overview.app_version, "0.1.5");
    assert_eq!(overview.app_data_path, "C:/data/Agent Switchboard");
    assert!(matches!(
        overview.build_mode,
        RuntimeBuildMode::Debug | RuntimeBuildMode::Release
    ));
    assert!(!overview.platform.is_empty());
    assert!(!overview.architecture.is_empty());
    assert_eq!(
        overview.transport,
        RuntimeTransport::WebDevelopment {
            host: "127.0.0.1".to_string(),
            port: 1422,
            health_status: 204,
        }
    );
    assert_eq!(
        serde_json::to_value(&overview.transport).expect("runtime transport serializes"),
        serde_json::json!({
            "kind": "webDevelopment",
            "host": "127.0.0.1",
            "port": 1422,
            "healthStatus": 204,
        })
    );
}
