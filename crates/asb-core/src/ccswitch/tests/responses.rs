use super::*;
use crate::contracts::{ResponsesOptions, ResponsesRequestMode};

#[test]
fn custom_responses_import_uses_standard_requests() {
    for (setting, _expected) in [
        ("", false),
    ] {
        let source = serde_json::json!({
            "auth": {"OPENAI_API_KEY": TOKEN},
            "config": format!("[model_providers.relay]\nbase_url = 'https://relay.example/openai'\nrequires_openai_auth = true\nwire_api = 'responses'\n{setting}"),
        });
        let result = map_row(&row("codex", "responses", "Responses", &source.to_string())).unwrap();
        assert_eq!(
            result.draft.responses_options,
            Some(ResponsesOptions {
                request_mode: ResponsesRequestMode::Standard,
            })
        );
        assert!(result.draft.validate().is_ok());
    }
}

#[test]
fn invalid_authentication_field_is_rejected() {
    let source = serde_json::json!({
        "auth": {"OPENAI_API_KEY": TOKEN},
        "config": "[model_providers.relay]\nbase_url = 'https://relay.example/openai'\nrequires_openai_auth = 'false'\n",
    });
    let skipped =
        map_row(&row("codex", "responses", "Responses", &source.to_string())).unwrap_err();
    assert!(skipped.reason.contains("requires_openai_auth 必须为布尔值"));
}
