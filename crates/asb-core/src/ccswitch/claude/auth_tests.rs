use super::*;
use crate::contracts::AuthenticationScheme;
use crate::ccswitch::mapping::map_row;
use serde_json::json;

fn row(meta: Value, env: Value) -> CcSwitchRow {
    CcSwitchRow {
        id: "managed-fixture".into(),
        app_type: "claude".into(),
        name: "托管 Claude".into(),
        settings_config: json!({"env":env}).to_string(),
        website_url: None,
        notes: None,
        meta: Some(meta.to_string()),
    }
}

#[test]
fn keyless_managed_rows_are_custom_gateway_profiles_not_official_routes() {
    for (kind, protocol, endpoint) in [
        (
            "github_copilot",
            UpstreamProtocol::AnthropicMessages,
            "https://api.githubcopilot.com",
        ),
        (
            "codex_oauth",
            UpstreamProtocol::Responses,
            "https://chatgpt.com/backend-api/codex",
        ),
        (
            "xai_oauth",
            UpstreamProtocol::Responses,
            "https://api.x.ai/v1",
        ),
    ] {
        let mapped = map_row(&row(json!({"providerType":kind}), json!({}))).unwrap();
        let CcSwitchProviderDraft::Claude(draft) = mapped.draft else {
            panic!("Claude draft");
        };
        assert_eq!(draft.route_mode, RouteMode::Custom);
        assert_eq!(draft.upstream_protocol, Some(protocol));
        assert_eq!(draft.base_url.as_deref(), Some(endpoint));
        assert!(draft.connection.requires_gateway());
        assert!(draft.api_key.is_empty());
        draft.validate().unwrap();
    }
}

#[test]
fn managed_account_pins_and_source_aliases_survive_without_placeholder_credentials() {
    let mapped = map_row(&row(
        json!({"providerType":"github_copilot", "githubAccountId":"42"}),
        json!({"ANTHROPIC_AUTH_TOKEN":"copilot_placeholder"}),
    ))
    .unwrap();
    let CcSwitchProviderDraft::Claude(draft) = mapped.draft else {
        panic!("Claude");
    };
    assert_eq!(
        draft.connection.auth_binding.unwrap().account_id.as_deref(),
        Some("42")
    );
    assert!(draft.api_key.is_empty());
    assert!(!mapped
        .warnings
        .iter()
        .any(|warning| warning.contains("githubAccountId")));
}

#[test]
fn malformed_or_mismatched_authentication_never_silently_selects_a_default_account() {
    for meta in [
        json!({"providerType":"xai_oauth", "authBinding":true}),
        json!({"providerType":"xai_oauth", "authBinding":{"source":"managed_account", "authProvider":"github_copilot"}}),
        json!({"providerType":"codex_oauth", "apiFormat":"anthropic"}),
        json!({"providerType":"other_oauth"}),
    ] {
        assert!(map_row(&row(meta, json!({}))).is_err());
    }
}

#[test]
fn source_only_protocol_and_bearer_aliases_are_normalized_instead_of_becoming_anthropic() {
    let mut source = row(
        json!({}),
        json!({"ANTHROPIC_BASE_URL":"https://api.example/v1", "ANTHROPIC_API_KEY":"fake-key"}),
    );
    let mut config: Value = serde_json::from_str(&source.settings_config).unwrap();
    config["api_format"] = json!("openai_chat");
    config["auth_mode"] = json!("bearer_only");
    source.settings_config = config.to_string();
    let mapped = map_row(&source).unwrap();
    let CcSwitchProviderDraft::Claude(draft) = mapped.draft else {
        panic!("Claude draft");
    };
    assert_eq!(
        draft.upstream_protocol,
        Some(UpstreamProtocol::ChatCompletions)
    );
    assert_eq!(draft.authentication, Some(AuthenticationScheme::Bearer));
    assert!(!mapped
        .warnings
        .iter()
        .any(|value| value.contains("api_format") || value.contains("auth_mode")));
}

#[test]
fn ambiguous_native_credentials_and_malformed_metadata_never_masquerade_as_normal_profiles() {
    let source = row(
        json!({}),
        json!({"ANTHROPIC_BASE_URL":"https://bedrock.example", "ANTHROPIC_AUTH_TOKEN":"private-fixture", "CLAUDE_CODE_USE_BEDROCK":"1"}),
    );
    let error = map_row(&source).unwrap_err();
    assert!(error.reason.contains("BEDROCK"));
    assert!(!error.reason.contains("private-fixture"));
    let source = row(
        json!([]),
        json!({"ANTHROPIC_BASE_URL":"https://api.example", "ANTHROPIC_AUTH_TOKEN":"private-fixture"}),
    );
    assert!(map_row(&source).is_err());
}
