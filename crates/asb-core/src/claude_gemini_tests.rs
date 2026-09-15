use super::*;

#[test]
fn model_and_stream_own_the_google_operation_path() {
    for base in [
        "https://google.test",
        "https://google.test/",
        "https://google.test/v1beta",
    ] {
        assert_eq!(
            request_endpoint(base, false, "models/gemini-3-flash", false).unwrap(),
            "https://google.test/v1beta/models/gemini-3-flash:generateContent"
        );
        assert_eq!(
            request_endpoint(base, false, "gemini-3-flash", true).unwrap(),
            "https://google.test/v1beta/models/gemini-3-flash:streamGenerateContent?alt=sse"
        );
        assert_eq!(
            models_endpoint(base, false).unwrap(),
            "https://google.test/v1beta/models"
        );
    }
    assert_eq!(
        request_endpoint("https://google.test/proxy/v1", false, "m", false).unwrap(),
        "https://google.test/proxy/v1/models/m:generateContent"
    );
}

#[test]
fn full_google_operation_is_rebound_to_selected_model_without_query_loss() {
    let base = "https://google.test/custom/models/old:generateContent?tenant=a%20b&alt=json";
    assert_eq!(
        request_endpoint(base, true, "new", true).unwrap(),
        "https://google.test/custom/models/new:streamGenerateContent?tenant=a+b&alt=sse"
    );
    assert_eq!(
        models_endpoint(base, true).unwrap(),
        "https://google.test/custom/models?tenant=a+b"
    );
    assert!(request_endpoint("https://google.test/arbitrary", true, "m", false).is_err());
}

#[test]
fn previews_cannot_be_sent_as_real_requests() {
    let preview = request_preview("https://google.test", false, None).unwrap();
    assert!(preview.contains("/{model}:generateContent"));
    for model in ["", "{model}", "..", "a/b", "a?key=x", "a#b", "a\\b", "m\n"] {
        assert!(
            request_endpoint("https://google.test", false, model, false).is_err(),
            "{model}"
        );
    }
    assert!(crate::endpoint::upstream_endpoint(
        "https://google.test",
        UpstreamProtocol::GeminiGenerateContent
    )
    .is_err());
}

#[test]
fn structured_oauth_is_never_sent_as_a_bearer_json_blob() {
    let raw = r#"{"access_token":"ya29.fake","refresh_token":"refresh-secret","client_id":"id","client_secret":"client-secret"}"#;
    assert_eq!(
        credential(raw, None).unwrap(),
        (AuthenticationScheme::Bearer, "ya29.fake".into())
    );
    assert_eq!(
        credential("api-fake", None).unwrap(),
        (AuthenticationScheme::XGoogApiKey, "api-fake".into())
    );
    assert_eq!(
        credential("api-fake", Some(AuthenticationScheme::Bearer))
            .unwrap()
            .0,
        AuthenticationScheme::Bearer
    );
    assert_eq!(
        credential("ya29.fake", None).unwrap().0,
        AuthenticationScheme::Bearer
    );
    for raw in [
        r#"{"refresh_token":"secret"}"#,
        r#"{"access_token":"secret","expiry_date":1}"#,
        r#"{"access_token":"secret\nheader"}"#,
    ] {
        let error = credential(raw, None).unwrap_err();
        assert!(!error.contains("secret"));
    }
    assert!(credential("key", Some(AuthenticationScheme::XApiKey)).is_err());
}

#[test]
fn gemini_native_preset_prepares_as_a_google_native_profile() {
    let draft = crate::claude_presets::prepare(
        "claude-preset-46",
        "google-api-key-fixture",
        &Default::default(),
        None,
    )
    .unwrap()
    .draft;
    assert_eq!(
        draft.upstream_protocol,
        Some(UpstreamProtocol::GeminiGenerateContent)
    );
    assert_eq!(
        draft.authentication,
        Some(AuthenticationScheme::XGoogApiKey)
    );
    assert_eq!(draft.api_key, "google-api-key-fixture");
    assert_eq!(
        draft.base_url.as_deref(),
        Some("https://generativelanguage.googleapis.com")
    );
    assert!(draft.connection.claude_native.is_none());
    let display = draft.display.as_ref().expect("preset carries its category");
    assert_eq!(display.category.as_deref(), Some("third_party"));
    assert!(display.icon.is_none() && display.created_at.is_none());
    assert!(draft
        .model
        .as_ref()
        .is_some_and(|model| model.starts_with("gemini-")));
    // The CLI cannot deliver an x-goog-api-key from settings.json, so this
    // profile always routes through the local conversion gateway.
    let profile = crate::contracts::ProviderProfile::from_draft("gemini-fixture".into(), draft);
    assert!(profile.requires_gateway());
    let plan = crate::contracts::SwitchPlan::through_gateway(
        profile,
        crate::ownership::default_client_settings(crate::contracts::AppKind::Claude),
        "http://127.0.0.1:47821".into(),
        "asb-claude-capability".into(),
    );
    let rendered = crate::adapter::render("{}", &plan).unwrap();
    assert!(rendered.contains(r#""ANTHROPIC_BASE_URL": "http://127.0.0.1:47821""#));
    assert!(rendered.contains(r#""ANTHROPIC_AUTH_TOKEN": "asb-claude-capability""#));
    assert!(!rendered.contains("google-api-key-fixture"));
    assert!(!rendered.contains("generativelanguage"));
}
