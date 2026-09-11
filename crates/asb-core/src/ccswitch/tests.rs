use super::*;

use crate::contracts::{
    ConfigValue, ModelOptions, ProviderDraft, RouteMode, SettingValue, UpstreamProtocol, UsageQuery,
};

// Mapping keeps credentials in the backend-only proposal so import can
// persist them; the scan-response boundary is tested at the scan layer.
const TOKEN: &str = "<placeholder>";

fn claude_draft(outcome: &CcSwitchProposal) -> &ProviderDraft {
    match &outcome.draft {
        CcSwitchProviderDraft::Claude(draft) => draft,
        CcSwitchProviderDraft::Codex(_) => panic!("expected a Claude import proposal"),
    }
}

fn row(app_type: &str, id: &str, name: &str, settings_config: &str) -> CcSwitchRow {
    CcSwitchRow {
        id: id.to_string(),
        app_type: app_type.to_string(),
        name: name.to_string(),
        settings_config: settings_config.to_string(),
        website_url: None,
        notes: None,
        meta: None,
    }
}

fn claude_custom() -> String {
    format!(
        r#"{{
                "env": {{
                    "ANTHROPIC_BASE_URL": "https://relay.internal",
                    "ANTHROPIC_AUTH_TOKEN": "{TOKEN}",
                    "ANTHROPIC_MODEL": "claude-x",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "opus-x",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "sonnet-x",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "haiku-x",
                    "CLAUDE_CODE_SUBAGENT_MODEL": "sub-x"
                }},
                "permissions": {{"defaultMode": "auto"}}
            }}"#
    )
}

fn enabled_script_meta(source: &str) -> String {
    serde_json::json!({
        "usage_script": {
            "enabled": true,
            "language": "javascript",
            "code": source
        }
    })
    .to_string()
}

#[test]
fn claude_custom_imports_its_api_key_and_routing() {
    let outcome = map_row(&row("claude", "id-1", "中继 A", &claude_custom())).unwrap();
    assert_eq!(outcome.key, "claude:id-1");
    assert_eq!(claude_draft(&outcome).api_key, TOKEN);
    assert_eq!(
        claude_draft(&outcome).base_url.as_deref(),
        Some("https://relay.internal")
    );
    assert!(outcome
        .warnings
        .contains(&"未导入: permissions".to_string()));
    assert!(!outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("ANTHROPIC_AUTH_TOKEN")));
}

#[test]
fn claude_api_key_alias_imports_without_an_unimported_warning() {
    let config = format!(
        r#"{{"env":{{"ANTHROPIC_BASE_URL":"https://relay.internal","ANTHROPIC_API_KEY":"{TOKEN}"}}}}"#
    );
    let outcome = map_row(&row("claude", "id-api-key", "兼容中继", &config)).unwrap();

    assert_eq!(claude_draft(&outcome).api_key, TOKEN);
    assert!(!outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("ANTHROPIC_API_KEY")));
}

#[test]
fn custom_usage_script_becomes_the_native_script_contract() {
    let mut source = row("claude", "usage-1", "带用量", &claude_custom());
    source.meta = Some(enabled_script_meta(
        r#"({
                request: {
                    url: "{{baseUrl}}/usage",
                    method: "GET",
                    headers: { Authorization: "Bearer {{apiKey}}" }
                },
                extractor: function(response) {
                    return [
                        { isValid: true, planName: "主套餐", remaining: response.main, unit: "USD" },
                        { isValid: true, planName: "副套餐", remaining: response.extra, unit: "USD" }
                    ];
                }
            })"#,
    ));

    let outcome = map_row(&source).expect("custom provider should map");
    let Some(UsageQuery::Script { source, .. }) = claude_draft(&outcome).usage_query.as_ref()
    else {
        panic!("usage script should be imported");
    };
    assert!(source.contains("const cc = ("));
    assert!(source.contains("cc.extractor(input.body)"));
    assert!(source.contains("planName"));
    assert!(!outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("usage_script")));
}

#[test]
fn disabled_or_template_usage_scripts_are_not_activated_on_import() {
    let mut disabled = row("claude", "usage-2", "禁用脚本", &claude_custom());
    disabled.meta = Some(
        serde_json::json!({
            "usage_script": {
                "enabled": false,
                "language": "javascript",
                "code": "({ request: {}, extractor: function() {} })"
            }
        })
        .to_string(),
    );
    let disabled = map_row(&disabled).expect("provider should still map");
    assert!(claude_draft(&disabled).usage_query.is_none());
    assert!(disabled
        .warnings
        .iter()
        .any(|warning| warning.contains("脚本已禁用")));

    let mut template = row("claude", "usage-3", "内建模板", &claude_custom());
    template.meta = Some(
        serde_json::json!({
            "usage_script": {
                "enabled": true,
                "language": "javascript",
                "code": "",
                "templateType": "balance"
            }
        })
        .to_string(),
    );
    let template = map_row(&template).expect("provider should still map");
    assert!(claude_draft(&template).usage_query.is_none());
    assert!(template
        .warnings
        .iter()
        .any(|warning| warning.contains("templateType")));
}

#[test]
fn independent_usage_script_inputs_are_not_mixed_into_the_provider() {
    let mut source = row("claude", "usage-4", "独立凭据", &claude_custom());
    source.meta = Some(
        serde_json::json!({
            "usage_script": {
                "enabled": true,
                "language": "javascript",
                "code": "({ request: {}, extractor: function() {} })",
                "accessToken": "<placeholder>"
            }
        })
        .to_string(),
    );

    let outcome = map_row(&source).expect("provider should still map");
    assert!(claude_draft(&outcome).usage_query.is_none());
    assert!(outcome
        .warnings
        .contains(&"未导入: meta.usage_script.accessToken".to_string()));
}

#[test]
fn claude_import_decodes_lowercase_one_m_model_markers_into_semantic_state() {
    let config = format!(
        r#"{{"env":{{"ANTHROPIC_BASE_URL":"https://relay.internal","ANTHROPIC_AUTH_TOKEN":"{TOKEN}","ANTHROPIC_MODEL":"claude-opus-4-1[1m]","ANTHROPIC_DEFAULT_SONNET_MODEL":"claude-sonnet-4-6[1m]","ANTHROPIC_DEFAULT_OPUS_MODEL":"claude-opus-4-1[1m]"}}}}"#
    );
    let outcome = map_row(&row("claude", "id-1m", "百万上下文", &config)).unwrap();

    assert_eq!(
        claude_draft(&outcome).model.as_deref(),
        Some("claude-opus-4-1")
    );
    let Some(ModelOptions::Claude(settings)) = claude_draft(&outcome).model_options.as_ref() else {
        panic!("Claude model settings should be imported");
    };
    assert!(settings.primary_one_m);
    assert_eq!(settings.sonnet_model.as_deref(), Some("claude-sonnet-4-6"));
    assert!(settings.sonnet_one_m);
    assert_eq!(settings.opus_model.as_deref(), Some("claude-opus-4-1"));
    assert!(settings.opus_one_m);
    assert!(claude_draft(&outcome).validate().is_ok());
}

#[test]
fn claude_import_normalizes_ccswitch_uppercase_one_m_model_markers() {
    let config = format!(
        r#"{{"env":{{"ANTHROPIC_BASE_URL":"https://relay.internal","ANTHROPIC_AUTH_TOKEN":"{TOKEN}","ANTHROPIC_MODEL":"claude-opus-4-1[1M]"}}}}"#
    );
    let outcome = map_row(&row("claude", "id-1m", "百万上下文", &config)).unwrap();
    assert_eq!(
        claude_draft(&outcome).model.as_deref(),
        Some("claude-opus-4-1")
    );
    let Some(ModelOptions::Claude(settings)) = claude_draft(&outcome).model_options.as_ref() else {
        panic!("Claude model settings should be imported");
    };
    assert!(settings.primary_one_m);
}

#[test]
fn claude_official_rows_become_credential_free_routes() {
    let config = format!(r#"{{"env": {{"ANTHROPIC_AUTH_TOKEN": "{TOKEN}"}}}}"#);
    let outcome = map_row(&row("claude", "id-2", "官方", &config)).unwrap();
    assert_eq!(claude_draft(&outcome).route_mode, RouteMode::Official);
    assert!(claude_draft(&outcome).api_key.is_empty());
    assert!(!format!("{outcome:?}").contains(TOKEN));
}

#[test]
fn claude_malformed_json_skips() {
    let outcome = map_row(&row("claude", "id-3", "坏档", "{oops")).unwrap_err();
    assert!(outcome.reason.contains("无法解析"));
}

#[test]
fn unsupported_client_skips() {
    let outcome = map_row(&row("gemini", "id-12", "双子", "{}")).unwrap_err();
    assert!(outcome.reason.contains("gemini"));
    let outcome = map_row(&row("claude-desktop", "id-13", "桌面", "{}")).unwrap_err();
    assert!(outcome.reason.contains("claude-desktop"));
}

#[test]
fn imported_claude_parameters_are_kept_and_excluded_from_unimported_warnings() {
    let mut config: serde_json::Value = serde_json::from_str(&claude_custom()).unwrap();
    config["effortLevel"] = "high".into();
    config["autoCompactEnabled"] = false.into();
    config["spinnerTipsEnabled"] = true.into();
    let outcome = map_row(&row(
        "claude",
        "parameters",
        "独立参数",
        &config.to_string(),
    ))
    .unwrap();
    assert_eq!(
        claude_draft(&outcome).parameters.value("effortLevel"),
        Some(&SettingValue::Explicit {
            value: ConfigValue::Str("high".into())
        })
    );
    assert_eq!(
        claude_draft(&outcome)
            .parameters
            .value("autoCompactEnabled"),
        Some(&SettingValue::Explicit {
            value: ConfigValue::Bool(false)
        })
    );
    assert!(!outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("effortLevel") || warning.contains("autoCompactEnabled")));
    assert!(outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("spinnerTipsEnabled")));
}

#[test]
fn invalid_ccswitch_parameter_values_are_rejected() {
    let config = serde_json::json!({"effortLevel": "unsupported"}).to_string();
    let skipped = map_row(&row("claude", "invalid-parameters", "无效参数", &config)).unwrap_err();
    assert!(skipped.reason.contains("effortLevel"));
}

#[path = "tests/codex.rs"]
mod codex;

#[test]
fn claude_openai_chat_meta_selects_the_upstream_protocol_and_keeps_a_bare_domain_root() {
    let config = serde_json::json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://integrate.api.nvidia.com",
            "ANTHROPIC_AUTH_TOKEN": "<placeholder>",
            "ANTHROPIC_MODEL": "moonshotai/kimi-k2.5"
        }
    })
    .to_string();
    let mut source = row("claude", "probe", "Nvidia", &config);
    source.meta = Some(serde_json::json!({"apiFormat": "openai_chat"}).to_string());
    let outcome = map_row(&source).expect("camelCase apiFormat maps");
    let draft = claude_draft(&outcome);
    assert_eq!(
        draft.upstream_protocol,
        Some(UpstreamProtocol::ChatCompletions)
    );
    assert_eq!(
        draft.base_url.as_deref(),
        Some("https://integrate.api.nvidia.com")
    );

    // The reference database stores this key in either spelling.
    let mut with_alias = row("claude", "probe-alias", "Alias", &config);
    with_alias.meta = Some(serde_json::json!({"api_format": "openai_chat"}).to_string());
    let outcome = map_row(&with_alias).expect("snake_case alias maps");
    let draft = claude_draft(&outcome);
    assert_eq!(
        draft.upstream_protocol,
        Some(UpstreamProtocol::ChatCompletions)
    );
    assert_eq!(
        draft.base_url.as_deref(),
        Some("https://integrate.api.nvidia.com")
    );
}
