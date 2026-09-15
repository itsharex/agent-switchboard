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
        CcSwitchProviderDraft::Codex(_) | CcSwitchProviderDraft::CodexOfficial(_) => {
            panic!("expected a Claude import proposal")
        }
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
        display: None,
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
    assert_eq!(
        claude_draft(&outcome).claude_fragment,
        serde_json::json!({"permissions": {"defaultMode": "auto"}})
            .as_object()
            .unwrap()
            .clone()
    );
    assert!(!outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("permissions")));
    assert!(!outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("ANTHROPIC_AUTH_TOKEN")));
}

#[test]
fn unmapped_source_settings_become_the_profile_fragment_with_named_losses() {
    let config = serde_json::json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://relay.internal",
            "ANTHROPIC_AUTH_TOKEN": TOKEN,
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": 1,
            "ASB_CLAUDE_COMMON_KEYS": "[]"
        },
        "permissions": {"allow": ["Read"]},
        "statusLine": {"command": "relay"},
        "includeCoAuthoredBy": false,
        "enabledPlugins": {"marketplace": true}
    })
    .to_string();
    let outcome = map_row(&row("claude", "id-frag", "片段中继", &config)).unwrap();
    let fragment = &claude_draft(&outcome).claude_fragment;
    assert_eq!(
        fragment["env"]["CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"],
        serde_json::json!("1")
    );
    assert_eq!(
        fragment["permissions"],
        serde_json::json!({"allow": ["Read"]})
    );
    assert_eq!(
        fragment["statusLine"],
        serde_json::json!({"command": "relay"})
    );
    assert_eq!(fragment["includeCoAuthoredBy"], serde_json::json!(false));
    assert!(!fragment.contains_key("enabledPlugins"));
    assert!(outcome
        .warnings
        .contains(&"未导入: enabledPlugins".to_string()));
    assert!(outcome
        .warnings
        .contains(&"未导入: env.ASB_CLAUDE_COMMON_KEYS".to_string()));
    assert!(!outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("permissions")));
}

#[test]
fn route_calibrated_context_defaults_fill_kimi_and_gpt56_codex_oauth_routes() {
    let kimi = serde_json::json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://api.kimi.com/coding",
            "ANTHROPIC_AUTH_TOKEN": TOKEN,
            "CLAUDE_CODE_MAX_CONTEXT_TOKENS": "100000"
        }
    })
    .to_string();
    let outcome = map_row(&row("claude", "kimi", "Kimi", &kimi)).unwrap();
    let fragment = &claude_draft(&outcome).claude_fragment;
    assert_eq!(
        fragment["env"]["CLAUDE_CODE_MAX_CONTEXT_TOKENS"],
        serde_json::json!("100000")
    );
    assert_eq!(
        fragment["env"]["CLAUDE_CODE_AUTO_COMPACT_WINDOW"],
        serde_json::json!("262144")
    );

    let gpt56 = serde_json::json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex",
            "ANTHROPIC_MODEL": "gpt-5.6-codex"
        }
    })
    .to_string();
    let outcome = map_row(&row("claude", "chatgpt", "ChatGPT", &gpt56)).unwrap();
    let fragment = &claude_draft(&outcome).claude_fragment;
    assert_eq!(
        fragment["env"]["CLAUDE_CODE_MAX_CONTEXT_TOKENS"],
        serde_json::json!("372000")
    );
    assert_eq!(
        fragment["env"]["CLAUDE_CODE_AUTO_COMPACT_WINDOW"],
        serde_json::json!("372000")
    );

    let gpt55 = serde_json::json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex",
            "ANTHROPIC_MODEL": "gpt-5.5-codex"
        }
    })
    .to_string();
    let outcome = map_row(&row("claude", "chatgpt-old", "ChatGPT 旧模型", &gpt55)).unwrap();
    assert!(claude_draft(&outcome).claude_fragment.get("env").is_none());
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

    // Built-in templates carry no code; the import synthesizes the vendored
    // native program instead of warning.
    let mut template = row("claude", "usage-3", "余额模板", &claude_custom());
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
    match &claude_draft(&template).usage_query {
        Some(crate::contracts::UsageQuery::Script { source, .. }) => {
            assert!(source.contains("/user/balance"));
            assert!(source.contains("api.deepseek.com"));
            assert!(source.contains("api.novita.ai"));
        }
        other => panic!("expected a synthesized balance script, got {other:?}"),
    }
    assert!(!template
        .warnings
        .iter()
        .any(|warning| warning.contains("templateType")));

    let mut general = row("claude", "usage-5", "通用模板", &claude_custom());
    general.meta = Some(
        serde_json::json!({
            "usage_script": {
                "enabled": true,
                "language": "javascript",
                "code": "",
                "templateType": "general"
            }
        })
        .to_string(),
    );
    let general = map_row(&general).expect("provider should still map");
    match &claude_draft(&general).usage_query {
        Some(crate::contracts::UsageQuery::Script { source, .. }) => {
            assert!(source.contains("/user/balance"));
        }
        other => panic!("expected a synthesized general script, got {other:?}"),
    }
}

#[test]
fn token_plan_templates_import_zhipu_and_skip_other_plans_precisely() {
    let zhipu_meta = |provider: &str| {
        serde_json::json!({
            "usage_script": {
                "enabled": true,
                "language": "javascript",
                "code": "",
                "templateType": "token_plan",
                "codingPlanProvider": provider
            }
        })
        .to_string()
    };

    let mut zhipu = row("claude", "plan-1", "智谱", &claude_custom());
    zhipu.meta = Some(zhipu_meta("zhipu"));
    let zhipu = map_row(&zhipu).expect("provider should still map");
    match &claude_draft(&zhipu).usage_query {
        Some(crate::contracts::UsageQuery::Script { source, .. }) => {
            assert!(source.contains("/api/monitor/usage/quota/limit"));
            assert!(source.contains("open.bigmodel.cn"));
        }
        other => panic!("expected a synthesized Zhipu script, got {other:?}"),
    }
    assert!(!zhipu
        .warnings
        .iter()
        .any(|warning| warning.contains("codingPlanProvider")));

    // Every data-plane coding-plan provider synthesizes its vendored program.
    for (provider, marker) in [
        ("kimi", "api.kimi.com/coding/v1/usages"),
        ("minimax", "coding_plan/remains"),
        ("zenmux", "usage_percentage"),
        ("opencode_go", "opencode.ai/zen/go/v1/usage"),
    ] {
        let mut source = row("claude", "plan-x", provider, &claude_custom());
        source.meta = Some(zhipu_meta(provider));
        let mapped = map_row(&source).expect("provider should still map");
        match &claude_draft(&mapped).usage_query {
            Some(crate::contracts::UsageQuery::Script { source, .. }) => {
                assert!(source.contains(marker), "{provider}: {source}");
            }
            other => panic!("expected a synthesized {provider} script, got {other:?}"),
        }
    }

    // The team plan bakes its source-configured identifiers and consumes them.
    let mut team = row("claude", "plan-team", "智谱团队", &claude_custom());
    team.meta = Some(
        serde_json::json!({
            "usage_script": {
                "enabled": true,
                "language": "javascript",
                "code": "",
                "templateType": "token_plan",
                "codingPlanProvider": "zhipu_team",
                "teamOrganizationId": "org-\"x",
                "teamProjectId": "proj_1"
            }
        })
        .to_string(),
    );
    let team = map_row(&team).expect("provider should still map");
    match &claude_draft(&team).usage_query {
        Some(crate::contracts::UsageQuery::Script { source, .. }) => {
            assert!(source.contains("type=2"));
            assert!(source.contains("bigmodel-organization"));
            assert!(source.contains("org-\\\"x"));
            assert!(source.contains("proj_1"));
        }
        other => panic!("expected a synthesized team script, got {other:?}"),
    }
    assert!(!team
        .warnings
        .iter()
        .any(|warning| warning.contains("teamOrganizationId")));

    // A team plan without its identifiers is skipped precisely.
    let mut team_missing = row("claude", "plan-team-2", "智谱团队缺配置", &claude_custom());
    team_missing.meta = Some(zhipu_meta("zhipu_team"));
    let team_missing = map_row(&team_missing).expect("provider should still map");
    assert!(claude_draft(&team_missing).usage_query.is_none());
    assert!(team_missing
        .warnings
        .iter()
        .any(|warning| warning.contains("teamOrganizationId 与 teamProjectId")));

    // Volcengine needs control-plane AccessKey credentials: not representable.
    let mut volcengine = row("claude", "plan-v", "火山", &claude_custom());
    volcengine.meta = Some(zhipu_meta("volcengine"));
    let volcengine = map_row(&volcengine).expect("provider should still map");
    assert!(claude_draft(&volcengine).usage_query.is_none());
    assert!(volcengine
        .warnings
        .iter()
        .any(|warning| warning.contains("AccessKey ID/Secret")));

    let mut unknown = row("claude", "plan-u", "未知", &claude_custom());
    unknown.meta = Some(zhipu_meta("mistral"));
    let unknown = map_row(&unknown).expect("provider should still map");
    assert!(claude_draft(&unknown).usage_query.is_none());
    assert!(unknown
        .warnings
        .iter()
        .any(|warning| warning.contains("未知 Coding Plan 供应商 mistral")));

    let mut missing = row("claude", "plan-3", "缺供应商", &claude_custom());
    missing.meta = Some(
        serde_json::json!({
            "usage_script": {
                "enabled": true,
                "language": "javascript",
                "code": "",
                "templateType": "token_plan"
            }
        })
        .to_string(),
    );
    let missing = map_row(&missing).expect("provider should still map");
    assert!(claude_draft(&missing).usage_query.is_none());
    assert!(missing
        .warnings
        .iter()
        .any(|warning| warning.contains("缺少 codingPlanProvider")));
}

#[test]
fn non_representable_usage_templates_skip_with_precise_reasons() {
    let skip = |template: &str| {
        let mut source = row("claude", "skip-1", "不可表达", &claude_custom());
        source.meta = Some(
            serde_json::json!({
                "usage_script": {
                    "enabled": true,
                    "language": "javascript",
                    "code": "",
                    "templateType": template
                }
            })
            .to_string(),
        );
        let mapped = map_row(&source).expect("provider should still map");
        let reason = mapped
            .warnings
            .iter()
            .find(|warning| warning.contains("meta.usage_script（"))
            .map(String::as_str)
            .unwrap_or_default()
            .to_string();
        (claude_draft(&mapped).usage_query.is_none(), reason)
    };

    let (skipped, reason) = skip("newapi");
    assert!(skipped);
    assert!(reason.contains("站点独立凭据"));
    let (skipped, reason) = skip("github_copilot");
    assert!(skipped);
    assert!(reason.contains("GitHub 登录凭据"));
    let (skipped, reason) = skip("official_subscription");
    assert!(skipped);
    assert!(reason.contains("官方登录档案"));
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

#[test]
fn claude_display_columns_are_carried_into_the_draft() {
    let mut source = row(
        "claude",
        "id-display",
        "展示列",
        r#"{"env":{"ANTHROPIC_BASE_URL":"https://relay.example/v1","ANTHROPIC_AUTH_TOKEN":"tok"}}"#,
    );
    source.display = Some(crate::contracts::ProviderDisplay {
        icon: Some("robot".into()),
        icon_color: Some("#1122ff".into()),
        category: Some("third_party".into()),
        created_at: Some(1_730_000_000_000),
    });
    let outcome = map_row(&source).unwrap();
    let draft = claude_draft(&outcome);
    let display = draft.display.as_ref().expect("display carried over");
    assert_eq!(display.icon.as_deref(), Some("robot"));
    assert_eq!(display.icon_color.as_deref(), Some("#1122ff"));
    assert_eq!(display.category.as_deref(), Some("third_party"));
    assert_eq!(display.created_at, Some(1_730_000_000_000));
    // Display metadata never qualifies as live configuration: two drafts that
    // differ only in display are the same routing identity.
    let mut twin = draft.clone();
    twin.display = None;
    let profile = crate::contracts::ProviderProfile::from_draft("display-test".into(), twin);
    assert!(!profile.draft_touches_live_configuration(&draft));
}
