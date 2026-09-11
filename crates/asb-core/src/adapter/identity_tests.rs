use super::matches_provider_identity;
use crate::contracts::{AppKind, ProviderProfile, RouteMode, SwitchPlan, UpstreamProtocol};
use crate::ownership::default_client_settings;

fn profile(app: AppKind, mode: RouteMode) -> ProviderProfile {
    ProviderProfile {
        id: "identity-test".into(),
        app,
        route_mode: mode,
        name: "Identity test".into(),
        model: Some("profile-model".into()),
        base_url: (mode == RouteMode::Custom).then(|| "https://example.test/v1".into()),
        api_key: if mode == RouteMode::Custom {
            "fixture-key".into()
        } else {
            String::new()
        },
        upstream_protocol: (mode == RouteMode::Custom).then_some(match app {
            AppKind::Codex => UpstreamProtocol::Responses,
            AppKind::Claude => UpstreamProtocol::AnthropicMessages,
        }),
        responses_options: ((mode == RouteMode::Custom).then_some(match app {
            AppKind::Codex => UpstreamProtocol::Responses,
            AppKind::Claude => UpstreamProtocol::AnthropicMessages,
        }) == Some(crate::contracts::UpstreamProtocol::Responses))
        .then_some(crate::contracts::ResponsesOptions {
            request_mode: crate::contracts::ResponsesRequestMode::Standard,
        }),
        max_output_tokens: None.into(),
        model_options: None,
        parameters: crate::ownership::default_provider_parameters(app),
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

fn plan(app: AppKind, mode: RouteMode) -> SwitchPlan {
    let profile = profile(app, mode);
    if app == AppKind::Codex && mode == RouteMode::Custom {
        SwitchPlan::through_gateway(
            profile,
            default_client_settings(app),
            "http://127.0.0.1:47821/codex/asb_codex_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/v1".into(),
            String::new(),
        )
    } else {
        SwitchPlan::direct(profile, default_client_settings(app))
    }
}

#[test]
fn codex_identity_is_provider_and_gateway_endpoint_only() {
    let plan = plan(AppKind::Codex, RouteMode::Custom);
    let text = "model_provider = 'openai'\nopenai_base_url = 'http://127.0.0.1:47821/codex/asb_codex_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/v1'\nmodel = 'externally-changed'";
    assert!(matches_provider_identity(text, &plan).unwrap());
    assert!(!matches_provider_identity(
        &text.replace("0123456789abcdef", "fedcba9876543210"),
        &plan
    )
    .unwrap());
    assert!(
        !matches_provider_identity(&text.replace("'openai'", "'agent_switchboard'"), &plan)
            .unwrap()
    );
    assert!(!matches_provider_identity(
        text,
        &SwitchPlan::direct(plan.profile, plan.client_settings)
    )
    .unwrap());
}

#[test]
fn official_identity_does_not_observe_authentication() {
    let official = plan(AppKind::Codex, RouteMode::Official);
    assert!(matches_provider_identity("model = 'changed'", &official).unwrap());
    assert!(
        !matches_provider_identity("openai_base_url = 'https://other.test'", &official).unwrap()
    );
}

#[test]
fn claude_identity_still_checks_credentials() {
    let plan = plan(AppKind::Claude, RouteMode::Custom);
    let text = r#"{"env":{"ANTHROPIC_BASE_URL":"https://example.test/v1","ANTHROPIC_API_KEY":"fixture-key"}}"#;
    assert!(matches_provider_identity(text, &plan).unwrap());
    assert!(!matches_provider_identity(&text.replace("fixture-key", "other-key"), &plan).unwrap());
    assert!(
        !matches_provider_identity(text, &self::plan(AppKind::Claude, RouteMode::Official))
            .unwrap()
    );
}
