use super::*;
use crate::contracts::{ResponsesOptions, ResponsesRequestMode};
use crate::test_support::CODEX_AUTH_JSON;

fn custom_config(capability: &str) -> String {
    format!("model_provider = 'relay'\n[model_providers.relay]\nname = 'Relay'\nbase_url = 'https://relay.example/custom/api'\nwire_api = 'responses'\nrequires_openai_auth = true\n{capability}")
}

#[test]
fn import_uses_the_standard_responses_request_contract() {
    for (setting, _expected) in [
        ("", false),
    ] {
        let text = custom_config(setting);
        let file = inspect(AppKind::Codex, "fixture.toml", Some(&text));
        let proposal = import_proposal(&file, Some(&text), Some(CODEX_AUTH_JSON)).unwrap();
        assert_eq!(
            proposal.draft.responses_options,
            Some(ResponsesOptions {
                request_mode: ResponsesRequestMode::Standard,
            })
        );
        assert_eq!(
            proposal.draft.base_url.as_deref(),
            Some("https://relay.example/custom/api")
        );
        assert!(proposal.draft.validate().is_ok());
    }
}

#[test]
fn builtin_external_endpoint_import_uses_standard_requests() {
    let text = "model_provider = 'openai'\nopenai_base_url = 'https://relay.example/custom/api'\n[features]\nresponses_websockets = true\n";
    let file = inspect(AppKind::Codex, "fixture.toml", Some(text));
    let DiscoveredState::Ok { managed, .. } = &file.state else {
        panic!("valid config")
    };
    assert!(!managed);
    let proposal = import_proposal(&file, Some(text), Some(CODEX_AUTH_JSON)).unwrap();
    assert_eq!(
        proposal.draft.responses_options,
        Some(ResponsesOptions {
            request_mode: ResponsesRequestMode::Standard,
        })
    );
}

#[test]
fn invalid_capability_authentication_or_endpoint_blocks_import() {
    for text in [
        custom_config("supports_websockets = true"),
        custom_config("env_key = 'ANOTHER_KEY'"),
        custom_config("").replace(
            "requires_openai_auth = true",
            "requires_openai_auth = false",
        ),
        custom_config("").replace("/custom/api", "/custom/api/responses"),
    ] {
        let file = inspect(AppKind::Codex, "fixture.toml", Some(&text));
        assert!(import_proposal(&file, Some(&text), Some(CODEX_AUTH_JSON)).is_none());
    }
}
