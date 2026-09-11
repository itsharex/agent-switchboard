use super::*;

use crate::adapter::codex::document::{item_at, item_repr};
use crate::contracts::{ChangeKind, ConfigValue, RouteMode, SettingValue, SwitchPlan};
use crate::contracts::{CodexModelSettings, ModelOptions, ProviderProfile, UpstreamProtocol};
use crate::ownership::{default_client_settings, default_provider_parameters};
use crate::ownership::{
    CODEX_PROVIDER_BASE_URL_KEY, CODEX_SUBAGENT_MODEL_KEY, CODEX_SUBAGENT_REASONING_EFFORT_KEY,
    CODEX_WEB_SEARCH_KEY,
};
use crate::test_support::CODEX_TOML;
use crate::AppKind;
use toml_edit::DocumentMut;

mod responses;

fn plan_b() -> SwitchPlan {
    SwitchPlan::through_gateway(
        ProviderProfile {
            id: "p2".into(),
            app: AppKind::Codex,
            route_mode: crate::contracts::RouteMode::Custom,
            name: "Relay B".into(),
            model: Some("gpt-5.2".into()),
            base_url: Some("https://relay-b.internal/v1".into()),
            api_key: "CODEX_RELAY_B_KEY".into(),
            upstream_protocol: Some(UpstreamProtocol::Responses),
            responses_options: Some(crate::contracts::ResponsesOptions {
                request_mode: crate::contracts::ResponsesRequestMode::Standard,
            }),
            max_output_tokens: None.into(),
            model_options: Some(ModelOptions::Codex(CodexModelSettings {
                context_window: Some(272_000),
            })),
            parameters: parameters_with("model_reasoning_effort", ConfigValue::Str("xhigh".into())),
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        },
        default_client_settings(AppKind::Codex),
        "http://127.0.0.1:47821/codex/asb_codex_abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd/v1".into(),
        String::new(),
    )
}

fn parameters_with(key: &str, value: ConfigValue) -> crate::contracts::SettingsValues {
    let mut parameters = default_provider_parameters(AppKind::Codex);
    parameters
        .settings
        .insert(key.to_string(), SettingValue::Explicit { value });
    parameters
}

#[test]
fn parses_valid_and_rejects_invalid_toml_with_line() {
    assert!(parse(CODEX_TOML).is_ok());
    let err = parse("model = \"x\"\nthreads = [unclosed\n").unwrap_err();
    assert!(err.line.is_some());
}

#[test]
fn preview_names_changed_keys_and_backup_target() {
    let preview = preview(CODEX_TOML, &plan_b(), "F:/backups").unwrap();

    let changed: Vec<&str> = preview.changes.iter().map(|c| c.key.as_str()).collect();
    assert!(changed.contains(&"model"));
    assert!(!changed.contains(&"model_provider"));
    assert!(changed.contains(&CODEX_PROVIDER_BASE_URL_KEY));
    assert!(changed.contains(&"model_context_window"));
    assert_eq!(preview.backup_dir, "F:/backups");
    assert_eq!(preview.target, "~/.codex/config.toml");
    // Host keys stay untouched — asserted on the rendered text in
    // render_preserves_host_content_and_comments.
}

#[test]
fn official_route_removes_managed_custom_keys_and_keeps_host_keys() {
    let mut plan = plan_b();
    plan.profile.route_mode = RouteMode::Official;
    plan.profile.model = None;
    plan.profile.base_url = None;
    plan.profile.api_key.clear();
    plan.profile.upstream_protocol = None;
    plan.profile.responses_options = None;
    plan.profile.model_options = None;

    let rendered = render(CODEX_TOML, &plan).expect("official render");
    assert!(!rendered.contains("[model_providers.OpenAi]"));
    assert!(!rendered.contains("openai_base_url"));
    assert!(rendered.contains("model_provider = \"openai\""));
    assert!(rendered.contains("threads = 8"));
}

#[test]
fn explicit_provider_parameters_write_their_value() {
    let current = "";
    let preview = preview(current, &plan_b(), "/b").unwrap();
    let change = preview
        .changes
        .iter()
        .find(|c| c.key == "model_reasoning_effort")
        .expect("explicit provider parameter must be written");
    assert_eq!(change.kind, ChangeKind::Set);
}

#[test]
fn cross_protocol_route_explicitly_disables_codex_web_search() {
    let mut plan = plan_b();
    plan.profile.upstream_protocol = Some(UpstreamProtocol::ChatCompletions);
    plan.profile.responses_options = None;
    plan.profile.parameters =
        parameters_with(CODEX_WEB_SEARCH_KEY, ConfigValue::Str("live".into()));

    for mut projection in [
        plan.clone(),
        SwitchPlan::through_gateway(
            plan.profile.clone(),
            plan.client_settings.clone(),
            plan.client_base_url().unwrap().into(),
            String::new(),
        ),
    ] {
        let preview = preview("", &projection, "/b").expect("cross-protocol preview");
        let change = preview
            .changes
            .iter()
            .find(|change| change.key == CODEX_WEB_SEARCH_KEY)
            .expect("effective web-search guard must appear in the preview");
        assert_eq!(change.after.as_deref(), Some("disabled"));
        let rendered = render("", &projection).expect("cross-protocol render");
        assert!(rendered.contains("web_search = \"disabled\""));
        assert_eq!(projection.profile.parameters, plan.profile.parameters);
        projection.profile.upstream_protocol = Some(UpstreamProtocol::Responses);
        projection.profile.responses_options = plan_b().profile.responses_options;
        assert!(render(&rendered, &projection)
            .unwrap()
            .contains("web_search = \"live\""));
    }
}

#[test]
fn switching_providers_applies_independent_parameters_and_preserves_client_preferences() {
    let mut first = plan_b();
    first.profile.parameters =
        parameters_with("model_reasoning_effort", ConfigValue::Str("high".into()));
    first.client_settings.settings.insert(
        "tui.animations".into(),
        SettingValue::Explicit {
            value: ConfigValue::Bool(false),
        },
    );
    let mut second = first.clone();
    second.profile.id = "independent-provider".into();
    second.profile.parameters =
        parameters_with("model_reasoning_effort", ConfigValue::Str("low".into()));

    let first_text = crate::adapter::render("threads = 8\n", &first).unwrap();
    let second_text = crate::adapter::render(&first_text, &second).unwrap();
    assert!(second_text.contains("model_reasoning_effort = \"low\""));
    assert!(second_text.contains("animations = false"));
    assert!(second_text.contains("threads = 8"));
    assert_eq!(
        first.profile.parameters.value("model_reasoning_effort"),
        Some(&SettingValue::Explicit {
            value: ConfigValue::Str("high".into())
        })
    );

    second.profile.parameters = default_provider_parameters(AppKind::Codex);
    let automatic = crate::adapter::render(&second_text, &second).unwrap();
    assert!(!automatic.contains("model_reasoning_effort"));
    assert!(automatic.contains("animations = false"));
    assert!(crate::adapter::render(&automatic, &first)
        .unwrap()
        .contains("model_reasoning_effort = \"high\""));
}

#[test]
fn subagent_model_and_reasoning_follow_the_selected_provider() {
    let mut first = plan_b();
    first.profile.parameters = parameters_with(
        CODEX_SUBAGENT_MODEL_KEY,
        ConfigValue::Str("relay-a-coder".into()),
    );
    first.profile.parameters.settings.insert(
        CODEX_SUBAGENT_REASONING_EFFORT_KEY.into(),
        SettingValue::Explicit {
            value: ConfigValue::Str("high".into()),
        },
    );
    let mut second = first.clone();
    second.profile.id = "provider-b".into();
    second.profile.parameters = parameters_with(
        CODEX_SUBAGENT_MODEL_KEY,
        ConfigValue::Str("relay-b-coder".into()),
    );
    second.profile.parameters.settings.insert(
        CODEX_SUBAGENT_REASONING_EFFORT_KEY.into(),
        SettingValue::Explicit {
            value: ConfigValue::Str("low".into()),
        },
    );

    let first_text = crate::adapter::render("[agents]\nenabled = true\n", &first).unwrap();
    assert!(first_text.contains("default_subagent_model = \"relay-a-coder\""));
    assert!(first_text.contains("default_subagent_reasoning_effort = \"high\""));
    let second_text = crate::adapter::render(&first_text, &second).unwrap();
    assert!(second_text.contains("default_subagent_model = \"relay-b-coder\""));
    assert!(second_text.contains("default_subagent_reasoning_effort = \"low\""));
    assert!(second_text.contains("enabled = true"));

    second.profile.parameters = default_provider_parameters(AppKind::Codex);
    let automatic = crate::adapter::render(&second_text, &second).unwrap();
    assert!(!automatic.contains("default_subagent_model"));
    assert!(!automatic.contains("default_subagent_reasoning_effort"));
    assert!(automatic.contains("enabled = true"));
}

#[test]
fn native_responses_route_preserves_codex_web_search_preference() {
    let mut plan = plan_b();
    plan.profile.parameters =
        parameters_with(CODEX_WEB_SEARCH_KEY, ConfigValue::Str("live".into()));

    assert!(render("", &plan)
        .expect("native Responses render")
        .contains("web_search = \"live\""));
}

#[test]
fn automatic_provider_parameters_remove_hand_set_lines() {
    let mut plan = plan_b();
    plan.profile.parameters = default_provider_parameters(AppKind::Codex);
    let current = "model_verbosity = \"low\"\n";
    let preview = preview(current, &plan, "/b").unwrap();
    let change = preview
        .changes
        .iter()
        .find(|c| c.key == "model_verbosity")
        .expect("automatic behavior removes the app-owned line");
    assert_eq!(change.kind, ChangeKind::Remove);
    let rendered = render(current, &plan).unwrap();
    assert!(!rendered.contains("model_verbosity"));

    // With the line already absent automatic behavior produces no diff entry.
    let clean = super::preview("", &plan, "/b").unwrap();
    assert!(!clean.changes.iter().any(|c| c.key == "model_verbosity"));
}

#[test]
fn gateway_capability_is_redacted_in_preview() {
    let preview = preview(CODEX_TOML, &plan_b(), "/b").unwrap();
    let change = preview
        .changes
        .iter()
        .find(|c| c.key == "openai_base_url")
        .unwrap();
    assert_eq!(change.before.as_deref(), Some(crate::redact::REDACTED));
    assert_eq!(change.after.as_deref(), Some(crate::redact::REDACTED));
    assert!(!format!("{preview:?}").contains("asb_codex_abcdef"));
}

#[test]
fn preview_is_pure_input_text_untouched() {
    let input = CODEX_TOML.to_string();
    let _ = preview(&input, &plan_b(), "/b").unwrap();
    assert_eq!(input, CODEX_TOML);
}

#[test]
fn render_preserves_host_content_and_comments() {
    let rendered = render(CODEX_TOML, &plan_b()).unwrap();

    assert!(rendered.contains("# host-owned Codex configuration (test sample)"));
    assert!(rendered.contains("threads = 8"));
    assert!(rendered.contains("history_persistence = \"save-all\""));
    assert!(rendered.contains("trusted = true"));
    assert!(rendered.contains("model = \"gpt-5.2\""));
    assert!(rendered.contains("model_provider = \"openai\""));
    assert!(rendered
        .contains("openai_base_url = \"http://127.0.0.1:47821/codex/asb_codex_abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd/v1\""));
    assert!(!rendered.contains("[model_providers.OpenAi]"));
    // Output stays valid TOML.
    rendered
        .parse::<DocumentMut>()
        .expect("rendered document must be valid TOML");
}

#[test]
fn render_keeps_profile_api_key_out_of_config_toml() {
    let rendered = render(CODEX_TOML, &plan_b()).unwrap();
    let doc = rendered.parse::<DocumentMut>().unwrap();
    assert!(item_at(&doc, "experimental_bearer_token").is_none());
    assert!(item_at(&doc, "model_providers.OpenAi.experimental_bearer_token").is_none());
    assert!(!rendered.contains("CODEX_RELAY_B_KEY"));
}

#[test]
fn projection_replaces_builtin_openai_override_and_preserves_other_tables() {
    let legacy = r#"
model_provider = "openai"
openai_base_url = "https://legacy.internal/v1"

[model_providers.gateway]
name = "Gateway"
base_url = "https://gateway.internal/v1"
wire_api = "responses"
"#;

    let rendered = render(legacy, &plan_b()).expect("render custom route");
    assert!(rendered.contains("model_provider = \"openai\""));
    assert!(rendered
        .contains("openai_base_url = \"http://127.0.0.1:47821/codex/asb_codex_abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd/v1\""));
    let doc = rendered.parse::<DocumentMut>().expect("valid TOML");
    assert!(item_at(&doc, "experimental_bearer_token").is_none());
    assert!(item_at(&doc, "model_providers.OpenAi.experimental_bearer_token").is_none());
    assert_eq!(
        item_at(&doc, "model_providers.gateway.base_url")
            .and_then(item_repr)
            .as_deref(),
        Some("https://gateway.internal/v1")
    );
}

#[test]
fn dotted_parent_conflict_is_rejected_without_overwriting_host_content() {
    let current = "tui = \"host-owned scalar\"\nthreads = 8\n";
    let mut plan = plan_b();
    plan.client_settings.settings.insert(
        "tui.animations".into(),
        SettingValue::Explicit {
            value: ConfigValue::Bool(false),
        },
    );

    let preview_error = preview(current, &plan, "/b").unwrap_err();
    assert!(preview_error.message.contains("父节点 tui"));
    let render_error = render(current, &plan).unwrap_err();
    assert!(render_error.message.contains("父节点 tui"));
    assert_eq!(current, "tui = \"host-owned scalar\"\nthreads = 8\n");
}

#[test]
fn render_preserves_host_owned_provider_tables() {
    let current = format!(
        "{CODEX_TOML}\n[model_providers.gateway]\nbase_url = \"https://gateway.internal/v1\"\n"
    );
    let rendered = render(&current, &plan_b()).unwrap();
    assert!(rendered.contains("[model_providers.gateway]"));
    assert!(rendered.contains("base_url = \"https://gateway.internal/v1\""));
}

#[test]
fn render_is_idempotent() {
    let once = render(CODEX_TOML, &plan_b()).unwrap();
    let twice = render(&once, &plan_b()).unwrap();
    assert_eq!(once, twice);
}

#[test]
fn parse_error_messages_are_scrubbed() {
    let err = parse("api_key = \"sk-live-0123456789abcdefghij\"\nx = [oops\n").unwrap_err();
    assert!(!err.message.contains("sk-live"));
}

#[test]
fn route_state_reads_managed_custom_provider_and_model() {
    let state = route_state(CODEX_TOML);
    assert_eq!(state.route_mode, RouteMode::Custom);
    assert_eq!(state.provider_name.as_deref(), Some("openai"));
    assert_eq!(state.model.as_deref(), Some("gpt-5.1"));
    assert_eq!(
        state.base_url.as_deref(),
        Some("http://127.0.0.1:47821/codex/asb_codex_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/v1")
    );
    assert!(state.scope_warnings.is_empty());
}

#[test]
fn route_state_defaults_to_official_when_model_provider_is_omitted() {
    assert_eq!(
        route_state("model = 'gpt-test'\nthreads = 8\n").route_mode,
        RouteMode::Official
    );
}

#[test]
fn route_state_reads_custom_provider_tables() {
    let external = r#"
model_provider = "gateway"

[model_providers.gateway]
name = "Gateway"
base_url = "https://gateway.internal/v1"
wire_api = "responses"
"#;
    let state = route_state(external);
    assert_eq!(state.route_mode, RouteMode::Custom);
    assert_eq!(state.provider_name.as_deref(), Some("gateway"));
    assert!(state.base_url.is_none());
    assert!(state.wire_api.is_none());
}

#[test]
fn route_state_reads_builtin_openai_endpoint_override() {
    let legacy = r#"
model_provider = "openai"
openai_base_url = "https://legacy.internal/v1"
"#;
    let state = route_state(legacy);
    assert_eq!(state.route_mode, RouteMode::Custom);
    assert_eq!(
        state.base_url.as_deref(),
        Some("https://legacy.internal/v1")
    );
    assert!(state.wire_api.is_none());
}

#[test]
fn route_state_warns_when_profiles_could_override_user_config() {
    let with_profiles = format!("{CODEX_TOML}\n[profiles.dev]\nmodel = \"gpt-4o\"\n");
    let state = route_state(&with_profiles);
    assert_eq!(state.scope_warnings.len(), 1);
    assert!(state.scope_warnings[0].contains("--profile"));
}
