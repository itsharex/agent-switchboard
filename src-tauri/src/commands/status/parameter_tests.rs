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
        authentication: None,
        app: AppKind::Claude,
        route_mode: RouteMode::Custom,
        name: "parameter owner".into(),
        parameters: default_provider_parameters(AppKind::Claude),
        base_url: Some("https://relay.example/v1".into()),
        connection: Default::default(),
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
            "effortLevel".into(),
            SettingValue::Explicit {
                value: ConfigValue::Str("high".into()),
            },
        );
        let record = store.create_provider(original.clone()).unwrap();
        let gateway = GatewayController::start(&state);
        let projected = gateway
            .project(&SwitchPlan::direct(
                record.profile.clone(),
                default_client_settings(AppKind::Claude),
            ))
            .unwrap();
        gateway.commit(&projected, || Ok(())).unwrap();
        let content = asb_core::adapter::render("{}", &projected.plan).unwrap();
        let target = state.target(AppKind::Claude).unwrap();
        std::fs::write(&target, &content).unwrap();
        let statuses = super::report::config_status_report(&state, &gateway).unwrap();
        let status = statuses
            .iter()
            .find(|status| status.app == AppKind::Claude)
            .unwrap();
        assert!(status
            .route
            .as_ref()
            .unwrap()
            .base_url
            .as_deref()
            .is_some_and(|base_url| base_url.starts_with("http://127.0.0.1:")));
        assert_eq!(
            status.active_profile_id.as_deref(),
            Some(record.profile.id.as_str())
        );

        store
            .record_config_write(ConfigWriteRecord {
                app: AppKind::Claude,
                profile_id: Some(record.profile.id.clone()),
                profile_name: Some(record.profile.name.clone()),
                content_hash: asb_switch::sha256_hex(&content),
                backup_id: "isolated-backup".into(),
                at: "2026-09-07T00:00:00Z".into(),
                operation: WriteOperation::Projection,
            })
            .unwrap();
        assert!(matches!(
            match_status_for(&state, Some(&gateway), AppKind::Claude, &content).unwrap(),
            MatchStatus::MatchesProfile { .. }
        ));
        original.parameters.settings.insert(
            "effortLevel".into(),
            SettingValue::Explicit {
                value: ConfigValue::Str("xhigh".into()),
            },
        );
        store
            .update_provider(&record.profile.id, original, &record.file_hash)
            .unwrap();
        gateway.shutdown();
        assert!(matches!(
            match_status_for(&state, Some(&gateway), AppKind::Claude, &content).unwrap(),
            MatchStatus::ProfileChanged { .. }
        ));
    }
}

/// The stored Codex official-login record owns the live badge on an official
/// route: the Codex branch falls back to it after the third-party matcher
/// finds no routed profile.
#[test]
fn codex_official_route_attributes_the_active_state_to_the_stored_record() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let store = state.configuration();
    let record = store.create_provider(official_draft()).unwrap();
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
        status.route.as_ref().unwrap().route_mode,
        RouteMode::Official
    );
    assert_eq!(
        status.active_profile_id.as_deref(),
        Some(record.profile.id.as_str())
    );
    gateway.shutdown();
}

fn official_draft() -> ProviderDraft {
    ProviderDraft {
        authentication: None,
        app: AppKind::Codex,
        route_mode: RouteMode::Official,
        name: "Codex 官方登录".into(),
        parameters: default_provider_parameters(AppKind::Codex),
        base_url: None,
        connection: Default::default(),
        api_key: String::new(),
        upstream_protocol: None,
        responses_options: None,
        max_output_tokens: None.into(),
        model: None,
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}
