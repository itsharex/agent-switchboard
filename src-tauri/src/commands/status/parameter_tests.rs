use super::report::match_status_for;
use crate::gateway::GatewayController;
use crate::local_state::LocalState;
use asb_core::contracts::{
    AppKind, ConfigValue, ConfigWriteRecord, MatchStatus, ProviderDraft, RouteMode, SettingValue,
    SwitchPlan, UpstreamProtocol, WriteOperation,
};
use asb_core::ownership::{default_client_settings, default_provider_parameters};

fn draft(protocol: UpstreamProtocol) -> ProviderDraft {
    ProviderDraft {
        app: AppKind::Codex,
        route_mode: RouteMode::Custom,
        name: "parameter owner".into(),
        parameters: default_provider_parameters(AppKind::Codex),
        base_url: Some("https://relay.example/v1".into()),
        api_key: "isolated-fixture".into(),
        upstream_protocol: Some(protocol),
        responses_options: (Some(protocol)
            == Some(asb_core::contracts::UpstreamProtocol::Responses))
        .then_some(asb_core::contracts::ResponsesOptions {
            request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
        }),
        max_output_tokens: None.into(),
        model: Some("test-model".into()),
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

#[test]
fn changing_a_provider_parameter_changes_match_status_for_direct_and_gateway_routes() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    for protocol in [
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let state = LocalState::from_root(directory.path().join("state"));
        let store = state.configuration();
        let mut original = draft(protocol);
        original.parameters.settings.insert(
            "model_reasoning_effort".into(),
            SettingValue::Explicit {
                value: ConfigValue::Str("high".into()),
            },
        );
        let record = store.create_provider(original.clone()).unwrap();
        let gateway = GatewayController::start(&state);
        let projected = gateway
            .project(&SwitchPlan::direct(
                record.profile.clone(),
                default_client_settings(AppKind::Codex),
            ))
            .unwrap();
        gateway.commit(&projected, || Ok(())).unwrap();
        let content = asb_core::adapter::render("", &projected.plan).unwrap();
        let target = state.target(AppKind::Codex).unwrap();
        std::fs::write(&target, &content).unwrap();
        let statuses = super::report::config_status_report(&state, &gateway).unwrap();
        let status = statuses
            .iter()
            .find(|status| status.app == AppKind::Codex)
            .unwrap();
        assert_eq!(
            status.route.as_ref().unwrap().base_url.as_deref(),
            Some(asb_core::redact::REDACTED)
        );
        assert_eq!(
            status.active_profile_id.as_deref(),
            Some(record.profile.id.as_str())
        );

        store
            .record_config_write(ConfigWriteRecord {
                app: AppKind::Codex,
                profile_id: Some(record.profile.id.clone()),
                profile_name: Some(record.profile.name.clone()),
                content_hash: asb_switch::sha256_hex(&content),
                backup_id: "isolated-backup".into(),
                at: "2026-09-07T00:00:00Z".into(),
                operation: WriteOperation::Projection,
            })
            .unwrap();
        assert!(matches!(
            match_status_for(&state, Some(&gateway), AppKind::Codex, &content).unwrap(),
            MatchStatus::MatchesProfile { .. }
        ));
        original.parameters.settings.insert(
            "model_reasoning_effort".into(),
            SettingValue::Explicit {
                value: ConfigValue::Str("xhigh".into()),
            },
        );
        store
            .update_provider(&record.profile.id, original, &record.file_hash)
            .unwrap();
        gateway.shutdown();
        assert!(matches!(
            match_status_for(&state, Some(&gateway), AppKind::Codex, &content).unwrap(),
            MatchStatus::ProfileChanged { .. }
        ));
    }
}
