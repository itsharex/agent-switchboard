use crate::adapter;
use crate::ccswitch::{map_row, CcSwitchProviderDraft, CcSwitchRow};
use crate::contracts::{AppKind, AuthenticationScheme, ModelOptions, ProviderProfile, SwitchPlan};
use crate::ownership::default_client_settings;
use serde_json::{json, Value};

fn imported(config: Value) -> ProviderProfile {
    let proposal = map_row(&CcSwitchRow {
        id: "claude-parity".into(),
        app_type: "claude".into(),
        name: "Claude parity".into(),
        settings_config: config.to_string(),
        meta: None,
        notes: None,
        website_url: None,
    })
    .unwrap();
    assert!(proposal.warnings.is_empty(), "{:?}", proposal.warnings);
    let CcSwitchProviderDraft::Claude(draft) = proposal.draft else {
        panic!("Claude proposal");
    };
    ProviderProfile::from_draft("claude-parity".into(), draft)
}

fn plan(profile: ProviderProfile) -> SwitchPlan {
    SwitchPlan::direct(profile, default_client_settings(AppKind::Claude))
}

#[test]
fn claude_import_preserves_each_authentication_header_through_activation() {
    for (key, opposite, scheme) in [
        (
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_API_KEY",
            AuthenticationScheme::Bearer,
        ),
        (
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            AuthenticationScheme::XApiKey,
        ),
    ] {
        let profile = imported(
            json!({"env":{"ANTHROPIC_BASE_URL":"https://relay.test",key:"fixture-secret"}}),
        );
        assert_eq!(profile.authentication, Some(scheme));
        let plan = plan(profile);
        let current = json!({"env":{opposite:"old-key"},"hooks":{"keep":"untouched"}}).to_string();
        let rendered = adapter::render(&current, &plan).unwrap();
        let root: Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(root["env"][key], "fixture-secret");
        assert!(root["env"].get(opposite).is_none());
        assert_eq!(root["hooks"]["keep"], "untouched");
        assert!(adapter::matches_provider_identity(&rendered, &plan).unwrap());
    }
}

#[test]
fn claude_all_model_roles_names_and_one_m_roundtrip_then_clear_for_official() {
    let profile = imported(
        json!({"model":"vendor-main", "availableModels":["vendor-main","vendor-fable"],"env":{
            "ANTHROPIC_BASE_URL":"https://relay.test", "ANTHROPIC_AUTH_TOKEN":"fixture-key",
            "ANTHROPIC_DEFAULT_FABLE_MODEL":"vendor-fable[1M]",
            "CLAUDE_CODE_SUBAGENT_MODEL":"vendor-agent[1M]",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME":"Small", "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME":"Balanced",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME":"Large", "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME":"Fast large"
        }}),
    );
    assert_eq!(profile.model.as_deref(), Some("vendor-main"));
    let Some(ModelOptions::Claude(settings)) = &profile.model_options else {
        panic!("Claude models");
    };
    assert_eq!(settings.fable_model.as_deref(), Some("vendor-fable"));
    assert!(settings.fable_one_m && settings.subagent_one_m);
    assert_eq!(settings.subagent_model.as_deref(), Some("vendor-agent"));
    let rendered = adapter::render(
        r#"{"hooks":{"keep":true},"env":{"HTTP_PROXY":"http://proxy.test"}}"#,
        &plan(profile),
    )
    .unwrap();
    let root: Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(
        root["env"]["ANTHROPIC_DEFAULT_FABLE_MODEL"],
        "vendor-fable[1m]"
    );
    assert_eq!(
        root["env"]["CLAUDE_CODE_SUBAGENT_MODEL"],
        "vendor-agent[1m]"
    );
    assert_eq!(
        root["env"]["ANTHROPIC_DEFAULT_FABLE_MODEL_NAME"],
        "Fast large"
    );
    let official = imported(json!({}));
    let restored: Value =
        serde_json::from_str(&adapter::render(&rendered, &plan(official)).unwrap()).unwrap();
    assert_eq!(restored["env"], json!({"HTTP_PROXY":"http://proxy.test"}));
    assert_eq!(restored["hooks"], json!({"keep":true}));
}

#[test]
fn claude_local_import_uses_the_same_model_and_authentication_contract() {
    let text = json!({"model":"vendor-main","env":{
        "ANTHROPIC_BASE_URL":"https://relay.test","ANTHROPIC_AUTH_TOKEN":"fixture-key",
        "ANTHROPIC_DEFAULT_FABLE_MODEL":"vendor-fable[1m]","CLAUDE_CODE_SUBAGENT_MODEL":"vendor-agent",
        "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME":"Fable label"
    }}).to_string();
    let file = crate::discovery::inspect(AppKind::Claude, "fixture/settings.json", Some(&text));
    let draft = crate::discovery::claude_import_proposal(&file, Some(&text))
        .unwrap()
        .draft;
    assert_eq!(draft.authentication, Some(AuthenticationScheme::Bearer));
    let Some(ModelOptions::Claude(settings)) = draft.model_options else {
        panic!("Claude options");
    };
    assert_eq!(settings.fable_model.as_deref(), Some("vendor-fable"));
    assert_eq!(settings.subagent_model.as_deref(), Some("vendor-agent"));
    assert!(settings.fable_one_m);
}

#[test]
fn claude_switch_without_optional_roles_removes_old_supplier_overrides() {
    let current = json!({"env":{
        "ANTHROPIC_DEFAULT_FABLE_MODEL":"old-fable","CLAUDE_CODE_SUBAGENT_MODEL":"old-agent",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME":"old-name","ANTHROPIC_DEFAULT_FABLE_MODEL_NAME":"old-fable-name",
        "HOST_ONLY":"keep"
    }}).to_string();
    let profile = imported(
        json!({"env":{"ANTHROPIC_BASE_URL":"https://relay.test","ANTHROPIC_API_KEY":"fixture-key"}}),
    );
    let root: Value =
        serde_json::from_str(&adapter::render(&current, &plan(profile)).unwrap()).unwrap();
    assert_eq!(
        root["env"],
        json!({"HOST_ONLY":"keep","ANTHROPIC_BASE_URL":"https://relay.test","ANTHROPIC_API_KEY":"fixture-key"})
    );
}
