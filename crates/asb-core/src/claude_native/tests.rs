use super::*;
use crate::{
    adapter,
    contracts::{AppKind, ProviderProfile, RouteMode, SwitchPlan},
    ownership::default_client_settings,
};
use serde_json::{json, Value};
fn plan(api_key: bool) -> SwitchPlan {
    let variables = BTreeMap::from([("AWS_REGION".into(), "us-west-2".into())]);
    let variables = if api_key {
        variables
    } else {
        BTreeMap::from([
            ("AWS_REGION".into(), "us-west-2".into()),
            ("AWS_ACCESS_KEY_ID".into(), "fake-access-id".into()),
            ("AWS_SECRET_ACCESS_KEY".into(), "fake-secret/+=".into()),
        ])
    };
    let draft = crate::claude_presets::prepare(
        if api_key {
            "claude-preset-88"
        } else {
            "claude-preset-87"
        },
        if api_key { "fake-bedrock-key" } else { "" },
        &variables,
        None,
    )
    .unwrap()
    .draft;
    SwitchPlan::direct(
        ProviderProfile::from_draft("native-fixture".into(), draft),
        default_client_settings(AppKind::Claude),
    )
}
fn official(mut plan: SwitchPlan) -> SwitchPlan {
    plan.profile.route_mode = RouteMode::Official;
    plan.profile.connection = Default::default();
    plan.profile.base_url = None;
    plan.profile.upstream_protocol = None;
    plan.profile.model = None;
    plan.profile.model_options = None;
    plan
}
#[test]
fn bedrock_presets_use_native_sdk_credentials_and_never_anthropic_headers() {
    for key in [false, true] {
        let plan = plan(key);
        assert!(!plan.profile.requires_gateway());
        assert!(plan.profile.api_key.is_empty());
        let rendered = adapter::render("{}", &plan).unwrap();
        let root: Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(root["env"]["CLAUDE_CODE_USE_BEDROCK"], "1");
        assert_eq!(root["env"]["AWS_REGION"], "us-west-2");
        assert_eq!(
            root["env"]["ANTHROPIC_BEDROCK_BASE_URL"],
            "https://bedrock-runtime.us-west-2.amazonaws.com"
        );
        assert!(root["env"].get("ANTHROPIC_BASE_URL").is_none());
        assert!(root["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
        assert!(root["env"].get("ANTHROPIC_API_KEY").is_none());
        assert_eq!(
            root["env"][if key {
                "AWS_BEARER_TOKEN_BEDROCK"
            } else {
                "AWS_SECRET_ACCESS_KEY"
            }],
            if key {
                "fake-bedrock-key"
            } else {
                "fake-secret/+="
            }
        );
        assert!(adapter::matches_provider_identity(&rendered, &plan).unwrap());
        let preview =
            serde_json::to_string(&adapter::preview("{}", &plan, "temporary/backups").unwrap())
                .unwrap();
        assert!(!preview.contains("fake-bedrock-key"));
        assert!(!preview.contains("fake-secret"));
        assert!(!preview.contains("fake-access-id"));
    }
}
#[test]
fn native_namespace_switches_are_clean_and_unclaimed_host_keys_are_preserved() {
    let current=json!({"env":{"AWS_ACCESS_KEY_ID":"old","AWS_SECRET_ACCESS_KEY":"old-secret","UNRELATED":"keep","GOOGLE_APPLICATION_CREDENTIALS":"host-file"},"mcpServers":{"host":{"command":"keep"}},"hooks":{"keep":true}}).to_string();
    let first = plan(true);
    let rendered = adapter::render(&current, &first).unwrap();
    let root: Value = serde_json::from_str(&rendered).unwrap();
    assert!(root["env"].get("AWS_ACCESS_KEY_ID").is_none());
    assert_eq!(root["env"]["GOOGLE_APPLICATION_CREDENTIALS"], "host-file");
    let restored = adapter::render(&rendered, &official(first)).unwrap();
    let root: Value = serde_json::from_str(&restored).unwrap();
    assert!(root["env"].get("AWS_BEARER_TOKEN_BEDROCK").is_none());
    assert!(root["env"].get(MANIFEST).is_none());
    assert_eq!(root["env"]["UNRELATED"], "keep");
    assert_eq!(root["mcpServers"]["host"]["command"], "keep");
    assert_eq!(root["hooks"]["keep"], true);
    let untouched = adapter::render(&current, &official(plan(false))).unwrap();
    let root: Value = serde_json::from_str(&untouched).unwrap();
    assert_eq!(root["env"]["AWS_SECRET_ACCESS_KEY"], "old-secret");
}
#[test]
fn vertex_adc_and_foundry_native_tokens_remain_distinct_from_api_key_profiles() {
    for env in [
        json!({"CLAUDE_CODE_USE_VERTEX":"1","CLOUD_ML_REGION":"global","ANTHROPIC_VERTEX_PROJECT_ID":"fixture-project","GOOGLE_APPLICATION_CREDENTIALS":"fixture.json"}),
        json!({"CLAUDE_CODE_USE_FOUNDRY":"1","ANTHROPIC_FOUNDRY_RESOURCE":"fixture-resource","ANTHROPIC_FOUNDRY_AUTH_TOKEN":"fake-entra-token"}),
    ] {
        let config = json!({"env":env,"model":"claude-sonnet-4-6"});
        let (native, base) = from_config(&config).unwrap().unwrap();
        let mut plan = plan(false);
        plan.profile.connection.claude_native = Some(native.clone());
        plan.profile.base_url = base;
        let rendered = adapter::render("{}", &plan).unwrap();
        let root: Value = serde_json::from_str(&rendered).unwrap();
        for (key, value) in native.environment {
            assert_eq!(root["env"][&key], value);
        }
        let discovered =
            crate::discovery::inspect(AppKind::Claude, "isolated/settings.json", Some(&rendered));
        let imported =
            crate::discovery::claude_import_proposal(&discovered, Some(&rendered)).unwrap();
        assert_eq!(
            imported.draft.connection.claude_native,
            plan.profile.connection.claude_native
        );
        assert_eq!(imported.draft.route_mode, RouteMode::Custom);
        assert!(imported.draft.api_key.is_empty());
    }
}
#[test]
fn invalid_namespaces_combinations_and_metadata_keep_the_original_file() {
    assert!(from_config(
        &json!({"env":{"CLAUDE_CODE_USE_BEDROCK":"1","CLAUDE_CODE_USE_VERTEX":"1"}})
    )
    .is_err());
    assert!(from_config(
        &json!({"env":{"CLAUDE_CODE_USE_BEDROCK":"1","AWS_ACCESS_KEY_ID":"fake"}})
    )
    .is_err());
    let mut native = plan(false);
    native.profile.connection.custom_user_agent = Some("unsupported HTTP override".into());
    assert!(native.profile.validate().is_err());
    let current = json!({"env":{MANIFEST:"[\"UNRELATED\"]","UNRELATED":"keep"}}).to_string();
    assert!(adapter::render(&current, &official(plan(false))).is_err());
    let plan = plan(true);
    let gateway = SwitchPlan::through_gateway(
        plan.profile,
        plan.client_settings,
        "http://127.0.0.1:1".into(),
        "fake-local-token".into(),
    );
    assert!(adapter::render("{}", &gateway).is_err());
}
