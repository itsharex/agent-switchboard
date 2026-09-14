use super::*;

#[test]
fn every_available_preset_prepares_through_the_real_claude_contract() {
    let presets = list().unwrap();
    assert_eq!(presets.len(), 90);
    let mut prepared = 0;
    for preset in presets {
        let variables = preset
            .variables
            .keys()
            .map(|key| (key.clone(), "fixture-endpoint".into()))
            .collect::<BTreeMap<_, _>>();
        let credential = if preset.authentication == "api_key" {
            "isolated-test-key"
        } else {
            ""
        };
        let result = prepare(&preset.id, credential, &variables, None);
        if preset.unavailable_reason.is_some() {
            assert!(result.is_err());
            continue;
        }
        let result = result.unwrap_or_else(|error| panic!("{}: {error}", preset.name));
        assert_eq!(result.draft.app, crate::AppKind::Claude);
        result.draft.validate().unwrap();
        prepared += 1;
    }
    assert_eq!(prepared, 90);
}

#[test]
fn variable_substitution_and_account_binding_are_validated_without_echoing_credentials() {
    let mut variables = BTreeMap::from([("ENDPOINT_ID".into(), "endpoint-fixture".into())]);
    let draft = prepare("claude-preset-68", "isolated-key", &variables, None)
        .unwrap()
        .draft;
    assert!(draft
        .base_url
        .unwrap()
        .contains("/endpoints/endpoint-fixture/"));
    variables.insert("ENDPOINT_ID".into(), "bad/path".into());
    assert!(prepare("claude-preset-68", "isolated-key", &variables, None).is_err());
    let draft = prepare("claude-preset-80", "", &BTreeMap::new(), Some("42"))
        .unwrap()
        .draft;
    assert!(draft.api_key.is_empty());
    assert!(draft.connection.requires_gateway());
    assert_eq!(
        draft.connection.auth_binding.unwrap().account_id.as_deref(),
        Some("42")
    );
    assert!(prepare(
        "claude-preset-80",
        "must-not-be-saved",
        &BTreeMap::new(),
        None
    )
    .is_err());
}

#[test]
fn model_endpoint_overrides_remain_executable_in_the_prepared_draft() {
    let draft = prepare("claude-preset-89", "isolated-key", &BTreeMap::new(), None)
        .unwrap()
        .draft;
    assert_eq!(
        draft.connection.claude_models_url.as_deref(),
        Some("https://api.jiekou.ai/openai/v1/models")
    );
    let endpoint = crate::endpoint::models_endpoint_for_connection(
        draft.base_url.as_ref().unwrap(),
        draft.upstream_protocol.unwrap(),
        &draft.connection,
    )
    .unwrap();
    assert_eq!(endpoint, "https://api.jiekou.ai/openai/v1/models");
}

#[test]
fn haiku_one_m_is_rendered_canonically_and_cleared_when_returning_to_official() {
    use crate::{
        adapter, ownership::default_client_settings, AppKind, ProviderProfile, SwitchPlan,
    };
    use serde_json::{json, Value};
    let preset = list()
        .unwrap()
        .into_iter()
        .find(|preset| preset.name == "MiniMax")
        .unwrap();
    let draft = prepare(&preset.id, "fake-key", &BTreeMap::new(), None)
        .unwrap()
        .draft;
    let plan = SwitchPlan::direct(
        ProviderProfile::from_draft("claude-model-test".into(), draft),
        default_client_settings(AppKind::Claude),
    );
    let initial = json!({"env":{"UNRELATED_HOST":"keep"},"customHost":{"keep":true}}).to_string();
    let text = adapter::render(&initial, &plan).unwrap();
    let rendered: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        rendered["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"],
        "MiniMax-M3[1m]"
    );
    assert_eq!(rendered["env"]["UNRELATED_HOST"], "keep");
    let official = prepare("claude-preset-01", "", &BTreeMap::new(), None)
        .unwrap()
        .draft;
    let plan = SwitchPlan::direct(
        ProviderProfile::from_draft("official-model-test".into(), official),
        default_client_settings(AppKind::Claude),
    );
    let restored: Value = serde_json::from_str(&adapter::render(&text, &plan).unwrap()).unwrap();
    assert!(restored["env"]
        .get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
        .is_none());
    assert_eq!(restored["customHost"], json!({"keep":true}));
}
