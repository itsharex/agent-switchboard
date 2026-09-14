use super::*;
use crate::{
    adapter,
    contracts::{ProviderProfile, RouteMode, SwitchPlan},
    ownership::default_client_settings,
    AppKind,
};
use serde_json::json;
fn settings(text: &str) -> crate::contracts::SettingsValues {
    adapter::parse_client_settings(AppKind::Claude, text).unwrap()
}
fn plan(text: &str) -> SwitchPlan {
    let mut profile:ProviderProfile=serde_json::from_value(json!({"id":"fixture","app":"claude","routeMode":"official","name":"Claude 官方登录","model":null,"baseUrl":null,"apiKey":"","authentication":null,"upstreamProtocol":null,"responsesOptions":null,"maxOutputTokens":null,"modelOptions":null,"parameters":crate::ownership::default_provider_parameters(AppKind::Claude),"websiteUrl":null})).unwrap();
    profile.route_mode = RouteMode::Official;
    SwitchPlan::direct(profile, settings(text))
}
#[test]
fn visual_fields_and_extra_json_roundtrip_without_duplicate_ownership() {
    let value = settings(
        r#"{"spinnerTipsEnabled":false,"env":{"DISABLE_TELEMETRY":"1","HTTP_PROXY":"http://127.0.0.1:1"},"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo test"}]}]},"permissions":{"allow":["Read"]}}"#,
    );
    assert!(!value.claude_extra.contains_key("spinnerTipsEnabled"));
    let text = adapter::render_client_settings(AppKind::Claude, &value).unwrap();
    assert!(!text.contains(MANIFEST));
    assert_eq!(settings(&text), value);
    assert_eq!(value.claude_extra["env"]["DISABLE_TELEMETRY"], "1");
}
#[test]
fn switching_applies_common_leaves_and_preserves_unowned_neighbours() {
    let initial=json!({"env":{"HOST":"keep"},"permissions":{"deny":["Bash(rm *)"]},"hooks":{"Other":["keep"]}}).to_string();
    let plan =
        plan(r#"{"env":{"COMMON_TOKEN":"fake-common-secret"},"permissions":{"allow":["Read"]}}"#);
    let rendered = adapter::render(&initial, &plan).unwrap();
    let value: Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(value["env"]["HOST"], "keep");
    assert_eq!(value["env"]["COMMON_TOKEN"], "fake-common-secret");
    assert_eq!(value["permissions"]["deny"], json!(["Bash(rm *)"]));
    assert_eq!(value["hooks"]["Other"], json!(["keep"]));
    let preview =
        serde_json::to_string(&adapter::preview(&initial, &plan, "temporary/backups").unwrap())
            .unwrap();
    assert!(!preview.contains("fake-common-secret"));
    let mut cleared = plan;
    cleared.client_settings = default_client_settings(AppKind::Claude);
    let next = adapter::render(&rendered, &cleared).unwrap();
    let value: Value = serde_json::from_str(&next).unwrap();
    assert!(value["env"].get("COMMON_TOKEN").is_none());
    assert!(value["permissions"].get("allow").is_none());
    assert_eq!(value["permissions"]["deny"], json!(["Bash(rm *)"]));
    let changes = adapter::owned_diff(AppKind::Claude, &next, &rendered).unwrap();
    assert!(changes.iter().any(|c| c.key == "/env/COMMON_TOKEN"));
}
#[test]
fn common_config_is_claude_client_only_and_reserved_names_cannot_be_claimed() {
    let values = settings(r#"{"env":{"DISABLE_TELEMETRY":"1"}}"#);
    assert!(values.validate_client_settings(AppKind::Codex).is_err());
    assert!(values
        .validate_provider_parameters(AppKind::Claude)
        .is_err());
    for text in [
        r#"{"model":"private"}"#,
        r#"{"alwaysThinkingEnabled":true}"#,
        r#"{"env":{"ANTHROPIC_API_KEY":"private"}}"#,
        r#"{"env":{"AWS_SECRET_ACCESS_KEY":"private"}}"#,
        r#"{"env":{"ASB_CLAUDE_COMMON_KEYS":"[]"}}"#,
        r#"{"mcpServers":{"x":{"command":"private"}}}"#,
        r#"{"enabledPlugins":{"private":true}}"#,
        r#"{"env":{"COUNT":1}}"#,
    ] {
        let error = adapter::parse_client_settings(AppKind::Claude, text).unwrap_err();
        assert!(!error.message.contains("private"), "{}", error.message);
    }
    let mut collision = default_client_settings(AppKind::Claude);
    collision.claude_extra = json!({"spinnerTipsEnabled":true})
        .as_object()
        .unwrap()
        .clone();
    assert!(collision.validate_client_settings(AppKind::Claude).is_err());
}
#[test]
fn corrupted_ownership_and_parent_conflicts_do_not_overwrite_host_data() {
    let forged =
        json!({"env":{MANIFEST:r#"["/env/ANTHROPIC_API_KEY"]"#,"ANTHROPIC_API_KEY":"host-secret"}})
            .to_string();
    assert!(apply(&forged, &Extra::new()).is_err());
    let extras = json!({"permissions":{"allow":["Read"]}})
        .as_object()
        .unwrap()
        .clone();
    assert!(apply(r#"{"permissions":"host value"}"#, &extras).is_err());
    assert!(adapter::preview("{}", &plan(r#"{"language":"zh-CN"}"#), "temporary/backups").is_ok());
}
#[test]
fn literal_dotted_and_escaped_field_names_are_not_reinterpreted_as_paths() {
    let extra = json!({"custom.field":{"a/b~c":[null,true,2]}})
        .as_object()
        .unwrap()
        .clone();
    let rendered = apply("{}", &extra).unwrap();
    let value: Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(value["custom.field"]["a/b~c"], json!([null, true, 2]));
    let next: Value = serde_json::from_str(&apply(&rendered, &Extra::new()).unwrap()).unwrap();
    assert!(next["custom.field"].get("a/b~c").is_none());
}
