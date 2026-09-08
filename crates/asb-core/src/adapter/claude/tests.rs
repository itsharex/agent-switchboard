use super::*;
use crate::contracts::{ChangeKind, ConfigValue, RouteMode, SettingValue, SwitchPlan};
use crate::contracts::{ClaudeModelSettings, ModelOptions, ProviderProfile, UpstreamProtocol};
use crate::ownership::{default_client_settings, default_provider_parameters};
use crate::test_support::CLAUDE_JSON;
use crate::AppKind;
use serde_json::Value as Json;

fn plan_b() -> SwitchPlan {
    SwitchPlan::direct(
        ProviderProfile {
            id: "c2".into(),
            app: AppKind::Claude,
            route_mode: crate::contracts::RouteMode::Custom,
            name: "Relay C".into(),
            model: Some("claude-opus-4".into()),
            base_url: Some("https://relay-c.internal".into()),
            api_key: "test-api-key".into(),
            upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
            responses_options: None,
            max_output_tokens: None.into(),
            model_options: Some(ModelOptions::Claude(ClaudeModelSettings {
                primary_one_m: false,
                haiku_model: Some("claude-haiku-4".into()),
                sonnet_model: None,
                sonnet_one_m: false,
                opus_model: None,
                opus_one_m: false,
                available_models: Some(vec![
                    "claude-opus-4".to_string(),
                    "claude-sonnet-4".to_string(),
                ]),
            })),
            parameters: default_provider_parameters(AppKind::Claude),
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        },
        default_client_settings(AppKind::Claude),
    )
}

#[test]
fn parses_valid_and_rejects_invalid_json_with_line() {
    assert!(parse(CLAUDE_JSON).is_ok());
    let err = parse("{\n  \"model\": oops\n}").unwrap_err();
    assert!(err.line.is_some());
}

#[test]
fn rejects_non_object_root() {
    let err = parse("[1, 2]").unwrap_err();
    assert!(err.message.contains("根节点"));
}

#[test]
fn preview_names_changes_warning_and_backup() {
    let preview = preview(CLAUDE_JSON, &plan_b(), "/backups").unwrap();

    let changed: Vec<&str> = preview.changes.iter().map(|c| c.key.as_str()).collect();
    assert!(changed.contains(&"model"));
    assert!(changed.contains(&"env.ANTHROPIC_BASE_URL"));
    assert!(changed.contains(&"env.ANTHROPIC_DEFAULT_HAIKU_MODEL"));
    assert!(changed.contains(&"availableModels"));
    assert!(changed.contains(&"env.ANTHROPIC_SMALL_FAST_MODEL"));
    assert_eq!(preview.backup_dir, "/backups");
    // Host keys stay untouched — asserted on the rendered text in
    // render_updates_owned_keys_and_keeps_host_blocks.
}

#[test]
fn official_route_removes_managed_custom_keys_and_keeps_host_keys() {
    let mut plan = plan_b();
    plan.profile.route_mode = RouteMode::Official;
    plan.profile.model = None;
    plan.profile.base_url = None;
    plan.profile.api_key.clear();
    plan.profile.upstream_protocol = None;
    plan.profile.model_options = None;

    let rendered = render(CLAUDE_JSON, &plan).expect("official render");
    assert!(!rendered.contains("ANTHROPIC_AUTH_TOKEN"));
    assert!(!rendered.contains("ANTHROPIC_BASE_URL"));
    assert!(!rendered.contains("ANTHROPIC_MODEL"));
    assert!(!rendered.contains("CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS"));
    assert!(rendered.contains("permissions"));
}

#[test]
fn cross_protocol_provider_disables_experimental_betas_and_native_provider_removes_it() {
    let mut profile = plan_b().profile;
    profile.upstream_protocol = Some(UpstreamProtocol::ChatCompletions);
    let routed = SwitchPlan::direct(profile, default_client_settings(AppKind::Claude));
    let rendered = render("{}", &routed).expect("routed Claude render");
    let parsed: Json = serde_json::from_str(&rendered).expect("routed Claude JSON");
    assert_eq!(
        parsed["env"]["CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS"],
        Json::String("1".into())
    );

    let native = render(
        r#"{"env":{"CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS":"1"}}"#,
        &plan_b(),
    )
    .expect("native Claude render");
    let parsed: Json = serde_json::from_str(&native).expect("native Claude JSON");
    assert!(parsed["env"]
        .get("CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS")
        .is_none());
}

#[test]
fn undeclared_tiers_are_removed_and_undeclared_lists_go_away() {
    let mut plan = plan_b();
    plan.profile.model_options = Some(ModelOptions::Claude(ClaudeModelSettings {
        primary_one_m: false,
        haiku_model: None,
        sonnet_model: Some("claude-sonnet-4".into()),
        sonnet_one_m: false,
        opus_model: None,
        opus_one_m: false,
        available_models: None,
    }));
    let current = r#"{
  "env": {
    "ANTHROPIC_BASE_URL": "https://relay-c.internal",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "old-sonnet",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "old-opus"
  },
  "availableModels": ["old-model"]
}"#;
    let rendered = render(current, &plan).unwrap();
    let parsed: Json = serde_json::from_str(&rendered).unwrap();
    assert_eq!(
        parsed["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"],
        Json::String("claude-sonnet-4".into())
    );
    assert!(parsed["env"].get("ANTHROPIC_DEFAULT_OPUS_MODEL").is_none());
    assert!(parsed.get("availableModels").is_none());
}

#[test]
fn deprecated_fast_model_key_is_cleaned_up() {
    let rendered = render(CLAUDE_JSON, &plan_b()).unwrap();
    assert!(!rendered.contains("ANTHROPIC_SMALL_FAST_MODEL"));
    assert!(!rendered.contains("claude-3-5-haiku-latest"));
}

#[test]
fn scalar_parameters_cannot_claim_the_provider_credential_slot() {
    let current = CLAUDE_JSON.replace(
        "\"ANTHROPIC_MODEL\": \"claude-sonnet-4\"",
        "\"ANTHROPIC_AUTH_TOKEN\": \"sk-live-old-secret\"",
    );
    let mut plan = plan_b();
    plan.profile.parameters.settings.insert(
        "env.ANTHROPIC_AUTH_TOKEN".into(),
        SettingValue::Explicit {
            value: ConfigValue::Str("sk-live-forbidden".into()),
        },
    );
    let err = crate::adapter::preview(&current, &plan, "/b").unwrap_err();
    assert!(err.message.contains("不属于"));

    let preview = preview(&current, &plan_b(), "/b").unwrap();
    // Anthropic Messages routes via x-api-key: the Bearer-era token key
    // is removed, and the credential lands on ANTHROPIC_API_KEY instead.
    let change = preview
        .changes
        .iter()
        .find(|change| change.key == "env.ANTHROPIC_AUTH_TOKEN")
        .expect("provider token change");
    assert_eq!(change.kind, ChangeKind::Remove);
    assert_eq!(change.before.as_deref(), Some(crate::redact::REDACTED));
    assert_eq!(change.after.as_deref(), None);
    let api_key_change = preview
        .changes
        .iter()
        .find(|change| change.key == "env.ANTHROPIC_API_KEY")
        .expect("provider api key change");
    assert_eq!(
        api_key_change.after.as_deref(),
        Some(crate::redact::REDACTED)
    );
    let rendered = render(&current, &plan_b()).unwrap();
    assert!(rendered.contains("test-api-key"));
    assert!(!rendered.contains("sk-live-old-secret"));
}

#[test]
fn explicit_ultracode_writes_its_own_key_and_automatic_removes_it() {
    let mut plan = plan_b();
    plan.profile.parameters.settings.insert(
        "ultracode".into(),
        SettingValue::Explicit {
            value: ConfigValue::Bool(true),
        },
    );
    let current = "{}";
    let rendered = render(current, &plan).unwrap();
    let parsed: Json = serde_json::from_str(&rendered).unwrap();
    assert_eq!(parsed["ultracode"], Json::Bool(true));

    // Automatic behavior leaves the file without the line.
    let mut defaulted = plan_b();
    defaulted
        .profile
        .parameters
        .settings
        .insert("ultracode".into(), SettingValue::Automatic);
    let with_line = "{\"ultracode\": true}";
    let rendered = render(with_line, &defaulted).unwrap();
    let parsed: Json = serde_json::from_str(&rendered).unwrap();
    assert!(parsed.get("ultracode").is_none());
}

#[test]
fn claude_provider_parameters_change_independently_of_client_settings() {
    let mut first = plan_b();
    first.profile.parameters.settings.insert(
        "effortLevel".into(),
        SettingValue::Explicit {
            value: ConfigValue::Str("high".into()),
        },
    );
    first.client_settings.settings.insert(
        "spinnerTipsEnabled".into(),
        SettingValue::Explicit {
            value: ConfigValue::Bool(false),
        },
    );
    let mut second = first.clone();
    second.profile.id = "independent-provider".into();
    second.profile.parameters.settings.insert(
        "effortLevel".into(),
        SettingValue::Explicit {
            value: ConfigValue::Str("low".into()),
        },
    );

    let first_text = crate::adapter::render("{}", &first).unwrap();
    let second_text = crate::adapter::render(&first_text, &second).unwrap();
    let parsed: Json = serde_json::from_str(&second_text).unwrap();
    assert_eq!(parsed["effortLevel"], "low");
    assert_eq!(parsed["spinnerTipsEnabled"], false);
    assert_eq!(
        first.profile.parameters.value("effortLevel"),
        Some(&SettingValue::Explicit {
            value: ConfigValue::Str("high".into())
        })
    );

    second.profile.parameters = default_provider_parameters(AppKind::Claude);
    let automatic = crate::adapter::render(&second_text, &second).unwrap();
    let parsed: Json = serde_json::from_str(&automatic).unwrap();
    assert!(parsed.get("effortLevel").is_none());
    assert_eq!(parsed["spinnerTipsEnabled"], false);
}

#[test]
fn env_parent_conflict_is_rejected_without_overwriting_host_content() {
    let current = r#"{
  "env": "host-owned scalar",
  "permissions": { "allow": ["Bash(*)"] }
}"#;

    let preview_error = preview(current, &plan_b(), "/b").unwrap_err();
    assert!(preview_error.message.contains("父节点 env"));
    let render_error = render(current, &plan_b()).unwrap_err();
    assert!(render_error.message.contains("父节点 env"));
    assert!(current.contains("host-owned scalar"));
}

#[test]
fn preview_is_pure_input_text_untouched() {
    let input = CLAUDE_JSON.to_string();
    let _ = preview(&input, &plan_b(), "/b").unwrap();
    assert_eq!(input, CLAUDE_JSON);
}

#[test]
fn render_updates_owned_keys_and_keeps_host_blocks() {
    let rendered = render(CLAUDE_JSON, &plan_b()).unwrap();
    let parsed: Json = serde_json::from_str(&rendered).unwrap();

    assert_eq!(parsed["model"], Json::String("claude-opus-4".into()));
    assert_eq!(
        parsed["env"]["ANTHROPIC_BASE_URL"],
        Json::String("https://relay-c.internal".into())
    );
    assert_eq!(
        parsed["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"],
        Json::String("claude-haiku-4".into())
    );
    assert_eq!(
        parsed["availableModels"],
        Json::Array(vec![
            Json::String("claude-opus-4".into()),
            Json::String("claude-sonnet-4".into())
        ])
    );
    // Host blocks survive.
    assert_eq!(
        parsed["permissions"]["allow"][0],
        Json::String("Bash(npm run test:*)".into())
    );
    assert!(parsed["statusLine"]["command"].is_string());
    // No credential material was written.
    let text = rendered.to_ascii_lowercase();
    assert!(!text.contains("sk-"));
    assert!(!text.contains("claude-relay-c"));
}

#[test]
fn render_writes_canonical_one_m_markers_for_supported_model_slots() {
    let mut plan = plan_b();
    plan.profile.model = Some("claude-opus-4-7".into());
    plan.profile.model_options = Some(ModelOptions::Claude(ClaudeModelSettings {
        primary_one_m: true,
        haiku_model: Some("claude-haiku-4".into()),
        sonnet_model: Some("claude-sonnet-4-6".into()),
        sonnet_one_m: true,
        opus_model: Some("claude-opus-4-7".into()),
        opus_one_m: true,
        available_models: None,
    }));

    let rendered = render(CLAUDE_JSON, &plan).unwrap();
    let parsed: Json = serde_json::from_str(&rendered).unwrap();
    assert_eq!(parsed["model"], Json::String("claude-opus-4-7[1m]".into()));
    assert_eq!(
        parsed["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"],
        Json::String("claude-sonnet-4-6[1m]".into())
    );
    assert_eq!(
        parsed["env"]["ANTHROPIC_DEFAULT_OPUS_MODEL"],
        Json::String("claude-opus-4-7[1m]".into())
    );
    assert_eq!(
        parsed["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"],
        Json::String("claude-haiku-4".into())
    );
}

#[test]
fn render_removes_env_model_override_when_primary_model_is_set() {
    let rendered = render(CLAUDE_JSON, &plan_b()).unwrap();
    let parsed: Json = serde_json::from_str(&rendered).unwrap();
    assert!(parsed["env"].get("ANTHROPIC_MODEL").is_none());
    assert_eq!(parsed["model"], Json::String("claude-opus-4".into()));
}

#[test]
fn render_creates_env_object_when_missing() {
    let current = "{\n  \"model\": \"m\"\n}";
    let rendered = render(current, &plan_b()).unwrap();
    let parsed: Json = serde_json::from_str(&rendered).unwrap();
    assert!(parsed["env"]["ANTHROPIC_BASE_URL"].is_string());
    assert_eq!(parsed["model"], Json::String("claude-opus-4".into()));
}

#[test]
fn render_is_idempotent() {
    let once = render(CLAUDE_JSON, &plan_b()).unwrap();
    let twice = render(&once, &plan_b()).unwrap();
    assert_eq!(once, twice);
}

#[test]
fn route_state_reads_model_base_url_and_mode() {
    let state = route_state(CLAUDE_JSON);
    assert_eq!(state.route_mode, RouteMode::Custom);
    assert_eq!(state.model.as_deref(), Some("claude-sonnet-4"));
    assert_eq!(state.base_url.as_deref(), Some("https://relay-a.internal"));
    assert!(state.provider_name.is_none());
}

#[test]
fn route_state_prefers_env_model_and_reports_official_without_endpoint() {
    let with_env_model = CLAUDE_JSON.replace(
        "\"ANTHROPIC_MODEL\": \"claude-sonnet-4\"",
        "\"ANTHROPIC_MODEL\": \"claude-opus-4\"",
    );
    assert_eq!(
        route_state(&with_env_model).model.as_deref(),
        Some("claude-opus-4")
    );

    let official = CLAUDE_JSON.replace(
        "    \"ANTHROPIC_BASE_URL\": \"https://relay-a.internal\",\n",
        "",
    );
    let state = route_state(&official);
    assert_eq!(state.route_mode, RouteMode::Official);
    assert!(state.base_url.is_none());
}
