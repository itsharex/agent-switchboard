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

#[test]
fn profile_fragment_tracks_its_own_manifest_and_cannot_double_own_shared_paths() {
    let mut with_fragment = plan(r#"{"env":{"HTTP_PROXY":"http://127.0.0.1:1"}}"#);
    with_fragment.profile.claude_fragment = json!({
        "statusLine": {"command": "first"},
        "env": {"CLAUDE_CODE_MAX_CONTEXT_TOKENS": "372000"}
    })
    .as_object()
    .unwrap()
    .clone();
    let initial = json!({"env":{"HOST":"keep"},"statusLine":{"command":"host"}}).to_string();
    let rendered = adapter::render(&initial, &with_fragment).unwrap();
    let value: Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(value["statusLine"]["command"], "first");
    assert_eq!(value["env"]["CLAUDE_CODE_MAX_CONTEXT_TOKENS"], "372000");
    assert_eq!(value["env"]["HTTP_PROXY"], "http://127.0.0.1:1");
    assert!(value["env"]
        .get(PROFILE_MANIFEST)
        .is_some_and(Value::is_string));
    let preview = adapter::preview(&initial, &with_fragment, "temporary/backups").unwrap();
    assert!(preview
        .changes
        .iter()
        .any(|change| change.key == "/env/CLAUDE_CODE_MAX_CONTEXT_TOKENS"));
    let changes = adapter::owned_diff(AppKind::Claude, &rendered, &initial).unwrap();
    assert!(changes
        .iter()
        .any(|change| change.key == "/env/CLAUDE_CODE_MAX_CONTEXT_TOKENS"));

    let cleared = plan(r#"{"env":{"HTTP_PROXY":"http://127.0.0.1:1"}}"#);
    let next = adapter::render(&rendered, &cleared).unwrap();
    let value: Value = serde_json::from_str(&next).unwrap();
    assert!(value.get("statusLine").is_none());
    assert!(value["env"].get("CLAUDE_CODE_MAX_CONTEXT_TOKENS").is_none());
    assert_eq!(value["env"]["HOST"], "keep");

    let leaf = json!({"attribution": {"commit": "conflict"}})
        .as_object()
        .unwrap()
        .clone();
    let parent = json!({"attribution": "flat"}).as_object().unwrap().clone();
    let shared_nested = plan(r#"{"attribution":{"commit":"","pr":""}}"#);
    for fragment in [leaf, parent] {
        let mut conflict = shared_nested.clone();
        conflict.profile.claude_fragment = fragment;
        assert!(adapter::render("{}", &conflict).is_err());
        assert!(adapter::preview("{}", &conflict, "temporary/backups").is_err());
    }
}

#[test]
fn import_fragment_coerces_env_scalars_and_names_rejections() {
    let config = json!({
        "env": {
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": 1,
            "ENABLE_TOOL_SEARCH": true,
            "ASB_CLAUDE_COMMON_KEYS": "[]",
            "NESTED": {"deep": true}
        },
        "permissions": {"allow": ["Read"]},
        "enabledPlugins": {"private": true},
        "includeCoAuthoredBy": false
    });
    let parameters = crate::ownership::default_provider_parameters(AppKind::Claude);
    let (kept, dropped) = import_fragment(&config, &parameters, &[]);
    assert_eq!(
        kept["env"]["CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"],
        json!("1")
    );
    assert_eq!(kept["env"]["ENABLE_TOOL_SEARCH"], json!("true"));
    assert_eq!(kept["permissions"], json!({"allow": ["Read"]}));
    assert_eq!(kept["includeCoAuthoredBy"], json!(false));
    assert!(!kept.contains_key("enabledPlugins"));
    let mut dropped = dropped;
    dropped.sort();
    assert_eq!(
        dropped,
        vec![
            "enabledPlugins".to_string(),
            "env.ASB_CLAUDE_COMMON_KEYS".to_string(),
            "env.NESTED".to_string(),
        ]
    );
}

/// The upstream provider-form quick toggles are plain settings keys; each one
/// must survive import into the profile fragment and reach the live file
/// through the profile manifest, then leave again with the profile.
#[test]
fn upstream_quick_toggles_round_trip_through_the_profile_fragment() {
    let config = json!({
        "attribution": {"commit": "", "pr": ""},
        "env": {
            "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1",
            "ENABLE_TOOL_SEARCH": "true",
            "CLAUDE_CODE_EFFORT_LEVEL": "max",
            "DISABLE_AUTOUPDATER": 1
        }
    });
    let parameters = crate::ownership::default_provider_parameters(AppKind::Claude);
    let (fragment, dropped) = import_fragment(&config, &parameters, &[]);
    assert!(dropped.is_empty(), "{dropped:?}");
    assert_eq!(fragment["attribution"], json!({"commit": "", "pr": ""}));
    assert_eq!(fragment["env"]["DISABLE_AUTOUPDATER"], json!("1"));
    validate(&fragment).unwrap();

    let mut plan = plan("{}");
    plan.profile.claude_fragment = fragment;
    let rendered = adapter::render("{}", &plan).unwrap();
    let live: Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(live["attribution"]["pr"], json!(""));
    assert_eq!(live["env"]["CLAUDE_CODE_EFFORT_LEVEL"], json!("max"));
    assert_eq!(live["env"]["CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"], json!("1"));
    assert_eq!(live["env"]["ENABLE_TOOL_SEARCH"], json!("true"));
    assert_eq!(live["env"]["DISABLE_AUTOUPDATER"], json!("1"));

    let official = self::plan("{}");
    let restored = adapter::render(&rendered, &official).unwrap();
    let after: Value = serde_json::from_str(&restored).unwrap();
    assert!(after.get("attribution").is_none());
    assert!(after.get("env").is_none_or(|env| env
        .as_object()
        .is_none_or(|env| !env.contains_key("CLAUDE_CODE_EFFORT_LEVEL"))));
}
