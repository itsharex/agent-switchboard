use super::*;

use crate::adapter::codex::document::{item_at, item_repr};
use crate::contracts::{ChangeKind, CommonSettingValue, ConfigValue, RouteMode, SwitchPlan};
use crate::contracts::{CodexModelSettings, ModelOptions, ProviderProfile, UpstreamProtocol};
use crate::ownership::default_common_settings;
use crate::ownership::CODEX_WEB_SEARCH_KEY;
use crate::test_support::CODEX_TOML;
use crate::AppKind;
use toml_edit::DocumentMut;

fn plan_b() -> SwitchPlan {
    SwitchPlan::direct(
        ProviderProfile {
            id: "p2".into(),
            app: AppKind::Codex,
            route_mode: crate::contracts::RouteMode::Custom,
            name: "Relay B".into(),
            model: Some("gpt-5.2".into()),
            base_url: Some("https://relay-b.internal/v1".into()),
            api_key: "CODEX_RELAY_B_KEY".into(),
            upstream_protocol: Some(UpstreamProtocol::Responses),
            max_output_tokens: None.into(),
            model_options: Some(ModelOptions::Codex(CodexModelSettings {
                context_window: Some(272_000),
            })),
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        },
        common_with("model_reasoning_effort", ConfigValue::Str("xhigh".into())),
    )
}

fn common_with(key: &str, value: ConfigValue) -> crate::contracts::CommonSettings {
    let mut common = default_common_settings(AppKind::Codex);
    common
        .settings
        .insert(key.to_string(), CommonSettingValue::Explicit { value });
    common
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
    assert!(changed.contains(&"openai_base_url"));
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
    plan.profile.model_options = None;

    let rendered = render(CODEX_TOML, &plan).expect("official render");
    assert!(!rendered.contains("[model_providers.OpenAi]"));
    assert!(!rendered.contains("openai_base_url"));
    assert!(rendered.contains("model_provider = \"openai\""));
    assert!(rendered.contains("threads = 8"));
}

#[test]
fn official_route_keeps_unknown_entries_in_the_managed_provider_table() {
    let current = format!(
            "{CODEX_TOML}\n[model_providers.OpenAi]\nname = \"Legacy Relay\"\nbase_url = \"https://legacy.internal/v1\"\nwire_api = \"responses\"\nexperimental_bearer_token = \"LEGACY_TOKEN\"\nhost_extension = \"preserve\"\n"
        );
    let mut plan = plan_b();
    plan.profile.route_mode = RouteMode::Official;
    plan.profile.model = None;
    plan.profile.base_url = None;
    plan.profile.api_key.clear();
    plan.profile.upstream_protocol = None;
    plan.profile.model_options = None;

    let rendered = render(&current, &plan).expect("official render");
    assert!(rendered.contains("[model_providers.OpenAi]"));
    assert!(rendered.contains("host_extension = \"preserve\""));
    assert!(!rendered.contains("experimental_bearer_token"));
    assert!(!rendered.contains("Legacy Relay"));
    assert!(!rendered.contains("https://legacy.internal/v1"));
}

#[test]
fn explicit_general_settings_write_their_value() {
    let current = "";
    let preview = preview(current, &plan_b(), "/b").unwrap();
    let change = preview
        .changes
        .iter()
        .find(|c| c.key == "model_reasoning_effort")
        .expect("explicit common value must be written");
    assert_eq!(change.kind, ChangeKind::Set);
}

#[test]
fn cross_protocol_route_explicitly_disables_codex_web_search() {
    let mut plan = plan_b();
    plan.profile.upstream_protocol = Some(UpstreamProtocol::ChatCompletions);
    plan.common = common_with(CODEX_WEB_SEARCH_KEY, ConfigValue::Str("live".into()));

    let preview = preview("", &plan, "/b").expect("cross-protocol preview");
    let change = preview
        .changes
        .iter()
        .find(|change| change.key == CODEX_WEB_SEARCH_KEY)
        .expect("effective web-search guard must appear in the preview");
    assert_eq!(change.after.as_deref(), Some("disabled"));
    assert!(render("", &plan)
        .expect("cross-protocol render")
        .contains("web_search = \"disabled\""));
}

#[test]
fn native_responses_route_preserves_codex_web_search_preference() {
    let mut plan = plan_b();
    plan.common = common_with(CODEX_WEB_SEARCH_KEY, ConfigValue::Str("live".into()));

    assert!(render("", &plan)
        .expect("native Responses render")
        .contains("web_search = \"live\""));
}

#[test]
fn automatic_general_settings_remove_hand_set_lines() {
    let mut plan = plan_b();
    plan.common = default_common_settings(AppKind::Codex);
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
fn preview_removes_legacy_bearer_token_and_redacts_it() {
    let current = CODEX_TOML.replace(
        "\n[projects.",
        "\nexperimental_bearer_token = \"sk-live-old-secret\"\n\n[projects.",
    );

    let preview = preview(&current, &plan_b(), "/b").unwrap();
    let change = preview
        .changes
        .iter()
        .find(|c| c.key == "experimental_bearer_token")
        .expect("legacy bearer token must be removed");
    assert_eq!(change.kind, ChangeKind::Remove);
    assert_eq!(change.before.as_deref(), Some(crate::redact::REDACTED));
    assert!(change.after.is_none());
    assert!(!format!("{preview:?}").contains("sk-live-old-secret"));
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
    assert!(rendered.contains("openai_base_url = \"https://relay-b.internal/v1\""));
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
fn projection_reuses_builtin_openai_route_and_preserves_other_tables() {
    let legacy = r#"
model_provider = "openai"
openai_base_url = "https://legacy.internal/v1"
experimental_bearer_token = "LEGACY_TOKEN"

[model_providers.gateway]
name = "Gateway"
base_url = "https://gateway.internal/v1"
wire_api = "responses"
"#;

    let rendered = render(legacy, &plan_b()).expect("render custom route");
    assert!(rendered.contains("openai_base_url = \"https://relay-b.internal/v1\""));
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
    plan.common = common_with("tui.animations", ConfigValue::Bool(false));

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
    assert_eq!(state.provider_name, None);
    assert_eq!(state.model.as_deref(), Some("gpt-5.1"));
    assert_eq!(
        state.base_url.as_deref(),
        Some("https://relay-a.internal/v1")
    );
    assert!(state.scope_warnings.is_empty());
}

#[test]
fn route_state_defaults_to_official_when_model_provider_is_omitted() {
    let implicit = CODEX_TOML
        .replace("model_provider = \"openai\"\n", "")
        .replace("openai_base_url = \"https://relay-a.internal/v1\"\n", "");
    assert_eq!(route_state(&implicit).route_mode, RouteMode::Official);
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
    assert_eq!(state.provider_name.as_deref(), Some("Gateway"));
    assert_eq!(
        state.base_url.as_deref(),
        Some("https://gateway.internal/v1")
    );
    assert_eq!(state.wire_api.as_deref(), Some("responses"));
}

#[test]
fn route_state_reads_builtin_openai_endpoint_override() {
    let legacy = r#"
model_provider = "openai"
openai_base_url = "https://legacy.internal/v1"
experimental_bearer_token = "LEGACY_TOKEN"
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
