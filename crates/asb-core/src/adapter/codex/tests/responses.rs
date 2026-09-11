use super::*;

#[test]
fn third_party_projection_uses_builtin_openai_without_credentials() {
    let plan = plan_b();
    let rendered = render("", &plan).unwrap();
    let doc = rendered.parse::<DocumentMut>().unwrap();
    assert_eq!(doc["model_provider"].as_str(), Some("openai"));
    assert_eq!(doc["openai_base_url"].as_str(), plan.client_base_url());
    assert!(doc.get("model_providers").is_none());
    assert!(!rendered.contains(&plan.profile.api_key));
    assert_eq!(
        super::super::route_state(&rendered)
            .provider_name
            .as_deref(),
        Some("openai")
    );
    let preview = preview("", &plan, "/backup").unwrap();
    assert_eq!(
        preview
            .changes
            .iter()
            .find(|change| change.key == "openai_base_url")
            .unwrap()
            .after
            .as_deref(),
        Some(crate::redact::REDACTED)
    );
}

#[test]
fn unprojected_third_party_cannot_render_or_preview() {
    let plan = plan_b();
    let direct = SwitchPlan::direct(plan.profile, plan.client_settings);
    assert!(render("", &direct).is_err());
    assert!(preview("", &direct, "/backup").is_err());
}

#[test]
fn official_switch_only_removes_managed_gateway_address() {
    let custom = plan_b();
    let current = render("threads = 8", &custom).unwrap();
    let mut profile = custom.profile;
    profile.route_mode = RouteMode::Official;
    profile.base_url = None;
    profile.api_key.clear();
    profile.upstream_protocol = None;
    profile.responses_options = None;
    let official = SwitchPlan::direct(profile, custom.client_settings);
    let rendered = render(&current, &official).unwrap();
    assert!(!rendered.contains("openai_base_url"));
    assert!(rendered.contains("model_provider = \"openai\""));
    assert!(rendered.contains("threads = 8"));
}

#[test]
fn gateway_projection_cannot_send_official_credentials_to_remote_hosts() {
    let plan = plan_b();
    for endpoint in [
        "https://third-party.test/codex/secret/v1",
        "http://127.0.0.1:47821/v1",
        "http://user@127.0.0.1:47821/codex/secret/v1",
        "http://127.0.0.1:47821/codex/secret/v1?other=1",
    ] {
        let invalid = SwitchPlan::through_gateway(
            plan.profile.clone(),
            plan.client_settings.clone(),
            endpoint.into(),
            String::new(),
        );
        assert!(render("", &invalid).is_err());
        assert!(super::super::render_gateway_base_url("", endpoint).is_err());
    }
}

#[test]
fn codex_gateway_capability_has_one_strict_url_shape() {
    let capability = format!("asb_codex_{}", "a".repeat(64));
    let valid = format!("http://127.0.0.1:47821/codex/{capability}/v1");
    assert!(super::super::is_gateway_base_url(&valid));

    for invalid in [
        format!(
            "http://127.0.0.1:47821/codex/asb_local_{}/v1",
            "a".repeat(64)
        ),
        format!(
            "http://127.0.0.1:47821/codex/asb_codex_{}/v1",
            "a".repeat(63)
        ),
        format!(
            "http://127.0.0.1:47821/codex/asb_codex_{}a/v1",
            "a".repeat(64)
        ),
        format!(
            "http://127.0.0.1:47821/codex/asb_codex_{}/v1",
            "g".repeat(64)
        ),
        format!("http://127.0.0.1:47821/codex/{capability}/v1?trace=1"),
        format!("http://127.0.0.1:47821/codex/{capability}/v1#fragment"),
        format!("http://user:password@127.0.0.1:47821/codex/{capability}/v1"),
        format!("http://localhost:47821/codex/{capability}/v1"),
        format!("http://127.0.0.2:47821/codex/{capability}/v1"),
        format!("https://127.0.0.1:47821/codex/{capability}/v1"),
        format!("http://127.0.0.1:47821/codex/{capability}/v1/extra"),
        format!("http://127.0.0.1:47821/codex/{capability}/v1/"),
    ] {
        assert!(
            !super::super::is_gateway_base_url(&invalid),
            "unexpectedly accepted {invalid}"
        );
    }
}
