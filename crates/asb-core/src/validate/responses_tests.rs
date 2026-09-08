use super::{tests::profile, ValidationError};
use crate::contracts::{
    AppKind, ResponsesOptions, ResponsesRequestMode, RouteMode, UpstreamProtocol,
};

#[test]
fn custom_responses_requires_declared_capabilities_for_each_client() {
    for app in [AppKind::Codex, AppKind::Claude] {
        let mut candidate = profile(app);
        candidate.responses_options = None;
        assert_eq!(
            candidate.validate(),
            Err(ValidationError::ResponsesRequiresOptions)
        );
        candidate.responses_options = Some(ResponsesOptions {

            request_mode: ResponsesRequestMode::Standard,
        });
        assert!(candidate.validate().is_ok());
    }
}

#[test]
fn only_responses_accepts_capability_settings() {
    let mut candidate = profile(AppKind::Codex);
    candidate.upstream_protocol = Some(UpstreamProtocol::ChatCompletions);
    assert_eq!(
        candidate.validate(),
        Err(ValidationError::UnexpectedResponsesOptions)
    );
    candidate.route_mode = RouteMode::Official;
    candidate.upstream_protocol = None;
    candidate.base_url = None;
    candidate.api_key.clear();
    candidate.model = None;
    candidate.model_options = None;
    assert_eq!(
        candidate.validate(),
        Err(ValidationError::OfficialRouteHasCustomFields)
    );
}

#[test]
fn minimal_requests_require_http_and_a_valid_api_root() {
    let mut candidate = profile(AppKind::Codex);
    candidate.responses_options = Some(ResponsesOptions {

        request_mode: ResponsesRequestMode::Minimal,
    });
    assert!(candidate.validate().is_ok());
    candidate.base_url = Some("https://relay.example/custom/api".into());
    assert!(candidate.validate().is_ok());
    candidate.base_url = Some("https://relay.example/custom/api/responses".into());
    assert!(matches!(
        candidate.validate(),
        Err(ValidationError::BadBaseUrl(_))
    ));
}

#[test]
fn url_validation_diagnostics_never_echo_embedded_credentials() {
    let mut candidate = profile(AppKind::Codex);
    for url in [
        "https://fixture-user:fixture-password@relay.example",
        "https://relay.example?key=fixture-secret",
    ] {
        candidate.base_url = Some(url.into());
        let error = candidate.validate().unwrap_err().to_string();
        assert!(!error.contains("fixture-"));
    }
}
