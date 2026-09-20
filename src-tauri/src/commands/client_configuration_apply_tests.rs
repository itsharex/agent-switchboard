use super::*;
use asb_core::{contracts::{ConfigValue, SettingValue}, ownership};
use serde_json::json;

#[test]
fn native_defaults_preserve_claude_extra_configuration() {
    let mut saved = ownership::default_client_settings(AppKind::Claude);
    saved.settings.insert(
        "spinnerTipsEnabled".into(),
        SettingValue::Explicit { value: ConfigValue::Bool(true) },
    );
    saved.claude_extra.insert("permissions".into(), json!({ "allow": ["git status"] }));

    let (reset, subagents) = ClientConfigurationResetKind::NativeDefaults
        .settings(AppKind::Claude, saved.clone())
        .expect("native reset");

    assert!(reset.settings.values().all(|value| matches!(value, SettingValue::Automatic)));
    assert_eq!(reset.claude_extra, saved.claude_extra);
    assert_eq!(subagents, None);
}

#[test]
fn clearing_extra_configuration_preserves_claude_standard_settings() {
    let mut saved = ownership::default_client_settings(AppKind::Claude);
    saved.settings.insert(
        "spinnerTipsEnabled".into(),
        SettingValue::Explicit { value: ConfigValue::Bool(false) },
    );
    saved.claude_extra.insert("permissions".into(), json!({ "allow": ["git status"] }));

    let (reset, subagents) = ClientConfigurationResetKind::ClearExtraConfiguration
        .settings(AppKind::Claude, saved.clone())
        .expect("clear extra configuration");

    assert_eq!(reset.settings, saved.settings);
    assert!(reset.claude_extra.is_empty());
    assert_eq!(subagents, None);
}

#[test]
fn clearing_extra_configuration_does_not_reproject_standard_settings() {
    let mut saved = ownership::default_client_settings(AppKind::Claude);
    saved.settings.insert(
        "spinnerTipsEnabled".into(),
        SettingValue::Explicit { value: ConfigValue::Bool(false) },
    );
    saved.claude_extra.insert("custom".into(), json!({ "value": "remove" }));
    let (reset, _) = ClientConfigurationResetKind::ClearExtraConfiguration
        .settings(AppKind::Claude, saved)
        .expect("clear extra configuration");
    let current = r#"{
  "spinnerTipsEnabled": true,
  "custom": { "value": "remove" },
  "env": { "ASB_CLAUDE_COMMON_KEYS": "[\"/custom/value\"]" }
}"#;

    let rendered = render_reset_configuration(
        AppKind::Claude,
        current,
        ClientConfigurationResetKind::ClearExtraConfiguration,
        &reset,
        None,
    )
    .expect("render extra clear");

    assert!(rendered.contains("\"spinnerTipsEnabled\": true"));
    assert!(!rendered.contains("\"custom\""));
    assert!(!rendered.contains("ASB_CLAUDE_COMMON_KEYS"));
}
#[test]
fn clearing_extra_configuration_rejects_codex() {
    let error = ClientConfigurationResetKind::ClearExtraConfiguration
        .settings(AppKind::Codex, ownership::default_client_settings(AppKind::Codex))
        .expect_err("Codex has no Claude extra configuration");

    assert_eq!(error.code, "client-configuration-rejected");
}

#[test]
fn deep_reset_matches_native_settings_intent() {
    let mut saved = ownership::default_client_settings(AppKind::Claude);
    saved.claude_extra.insert("custom".into(), json!({ "enabled": true }));

    let (reset, subagents) = ClientConfigurationResetKind::NativeDefaultsWithUnmanaged
        .settings(AppKind::Claude, saved.clone())
        .expect("deep reset");

    assert!(reset.settings.values().all(|value| matches!(value, SettingValue::Automatic)));
    assert_eq!(reset.claude_extra, saved.claude_extra);
    assert_eq!(subagents, None);
}

#[test]
fn deep_reset_removes_unmanaged_fields_but_keeps_provider_and_extra() {
    let mut saved = ownership::default_client_settings(AppKind::Claude);
    saved.claude_extra.insert("custom".into(), json!({ "enabled": true }));
    let (reset, _) = ClientConfigurationResetKind::NativeDefaultsWithUnmanaged
        .settings(AppKind::Claude, saved)
        .expect("deep reset");
    let current = r#"{
  "model": "claude-x",
  "spinnerTipsEnabled": true,
  "statusLine": { "type": "command" },
  "custom": { "enabled": true },
  "env": {
"ANTHROPIC_BASE_URL": "http://127.0.0.1:9",
"ASB_CLAUDE_COMMON_KEYS": "[\"/custom/enabled\"]",
"MY_TOOL_TOKEN": "v"
  }
}"#;

    let rendered = render_reset_configuration(
        AppKind::Claude,
        current,
        ClientConfigurationResetKind::NativeDefaultsWithUnmanaged,
        &reset,
        None,
    )
    .expect("render deep reset");

    assert!(rendered.contains("\"model\": \"claude-x\""));
    assert!(rendered.contains("ANTHROPIC_BASE_URL"));
    assert!(rendered.contains("custom"));
    assert!(rendered.contains("ASB_CLAUDE_COMMON_KEYS"));
    assert!(!rendered.contains("statusLine"));
    assert!(!rendered.contains("MY_TOOL_TOKEN"));
    assert!(!rendered.contains("spinnerTipsEnabled"));
}

#[test]
fn deep_reset_keeps_codex_provider_families_and_drops_host_tables() {
    let (reset, subagents) = ClientConfigurationResetKind::NativeDefaultsWithUnmanaged
        .settings(AppKind::Codex, ownership::default_client_settings(AppKind::Codex))
        .expect("deep reset");
    let current = "model = \"gpt-5\"\n\n[agents]\nenabled = true\n\n[tools]\nflag = true\n\n[model_providers.custom]\nbase_url = \"https://x\"\n";

    let rendered = render_reset_configuration(
        AppKind::Codex,
        current,
        ClientConfigurationResetKind::NativeDefaultsWithUnmanaged,
        &reset,
        subagents.as_ref(),
    )
    .expect("render deep reset");

    assert!(rendered.contains("model = \"gpt-5\""));
    assert!(rendered.contains("model_providers.custom"));
    assert!(!rendered.contains("[agents]"));
    assert!(!rendered.contains("[tools]"));
}
