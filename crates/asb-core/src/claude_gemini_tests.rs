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
