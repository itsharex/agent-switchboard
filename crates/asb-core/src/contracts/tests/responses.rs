use super::*;

fn responses_draft() -> ProviderDraft {
    let mut draft = super::classification_draft();
    draft.upstream_protocol = Some(UpstreamProtocol::Responses);
    draft.responses_options = Some(ResponsesOptions {
        request_mode: ResponsesRequestMode::Standard,
    });
    draft
}

#[test]
fn responses_capabilities_are_explicit_and_strict() {
    let expected = serde_json::json!({"requestMode": "standard"});
    assert_eq!(
        serde_json::to_value(responses_draft().responses_options).unwrap(),
        expected
    );
    for invalid in [
        serde_json::json!({"supportsWebsockets": false, "requestMode": "standard"}),
        serde_json::json!({"supportsWebsockets": false}),
        serde_json::json!({"supportsWebsockets": false, "requestMode": "compatibility"}),
        serde_json::json!({"supportsWebsockets": false, "requestMode": "standard", "extra": true}),
    ] {
        assert!(serde_json::from_value::<ResponsesOptions>(invalid).is_err());
    }
}

#[test]
fn responses_options_round_trip_and_reapply_an_active_provider() {
    let draft = responses_draft();
    let profile = ProviderProfile::from_draft("responses-contract".into(), draft.clone());
    let file = ProviderFile::from_profile(&profile, 100);
    let stored: ProviderFile = serde_json::from_value(serde_json::to_value(file).unwrap()).unwrap();
    assert_eq!(stored.into_profile(AppKind::Codex), profile);
    for changed in [ResponsesOptions {
        request_mode: ResponsesRequestMode::Minimal,
    }] {
        let mut edited = draft.clone();
        edited.responses_options = Some(changed);
        assert_eq!(
            classify_profile_save(Some(&profile), &edited, true),
            ProfileSaveKind::SaveAndApply
        );
        assert_eq!(
            classify_profile_save(Some(&profile), &edited, false),
            ProfileSaveKind::SaveOnly
        );
    }
}

#[test]
fn gateway_requirement_has_one_protocol_and_request_mode_owner() {
    let mut profile = ProviderProfile::from_draft("responses-route".into(), responses_draft());
    assert!(profile.requires_gateway());
    assert!(!profile.requires_protocol_translation());
    profile.responses_options.as_mut().unwrap().request_mode = ResponsesRequestMode::Minimal;
    assert!(profile.requires_gateway());
    profile.responses_options = None;
    profile.upstream_protocol = Some(UpstreamProtocol::ChatCompletions);
    assert!(profile.requires_gateway());
    profile.app = AppKind::Claude;
    profile.upstream_protocol = Some(UpstreamProtocol::AnthropicMessages);
    assert!(!profile.requires_gateway());
    profile.route_mode = RouteMode::Official;
    profile.upstream_protocol = None;
    assert!(!profile.requires_gateway());
}

#[test]
fn gateway_projection_retains_upstream_profile_and_hides_client_secrets() {
    let profile = ProviderProfile::from_draft("gateway".into(), responses_draft());
    let settings = crate::ownership::default_client_settings(AppKind::Codex);
    let plan = SwitchPlan::through_gateway(
        profile.clone(),
        settings,
        "http://127.0.0.1/codex/secret/v1".into(),
        "local-secret".into(),
    );
    assert_eq!(plan.profile, profile);
    assert!(plan.is_gateway());
    assert_eq!(plan.client_authentication(), None);
    assert_eq!(plan.client_api_key(), "");
    assert!(!format!("{plan:?}").contains("local-secret"));
    assert!(!format!("{plan:?}").contains("/codex/secret"));
}
