use super::*;

use crate::contracts::{
    AppKind, ClaudeModelSettings, CodexModelSettings, CommonSettingValue, ConfigValue,
    ModelOptions, RouteMode, UpstreamProtocol, UsageQuery,
};
use crate::ownership::default_common_settings;
use crate::validate::error::MAX_NOTES_LEN;

fn profile(app: AppKind) -> ProviderProfile {
    ProviderProfile {
        id: "p1".into(),
        app,
        route_mode: RouteMode::Custom,
        name: "Relay A".into(),
        model: Some("m-1".into()),
        base_url: Some("https://example.internal/v1".into()),
        api_key: "test-api-key".into(),
        upstream_protocol: Some(UpstreamProtocol::Responses),
        max_output_tokens: None.into(),
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

#[test]
fn rejects_empty_names() {
    let mut p = profile(AppKind::Codex);
    p.name = "  ".into();
    assert_eq!(p.validate(), Err(ValidationError::EmptyName));
}

#[test]
fn custom_mode_requires_a_base_url() {
    let mut p = profile(AppKind::Codex);
    p.base_url = None;
    assert_eq!(p.validate(), Err(ValidationError::CustomRequiresBaseUrl));
}

#[test]
fn missing_base_url_is_rejected_regardless_of_client() {
    let mut codex = profile(AppKind::Codex);
    codex.base_url = None;
    assert_eq!(
        codex.validate(),
        Err(ValidationError::CustomRequiresBaseUrl)
    );
    let mut claude = profile(AppKind::Claude);
    claude.base_url = None;
    assert_eq!(
        claude.validate(),
        Err(ValidationError::CustomRequiresBaseUrl)
    );
}

#[test]
fn rejects_non_http_base_url() {
    let mut p = profile(AppKind::Codex);
    p.base_url = Some("example.internal".into());
    assert!(matches!(p.validate(), Err(ValidationError::BadBaseUrl(_))));
}

#[test]
fn website_url_must_be_http_or_empty() {
    let mut p = profile(AppKind::Codex);
    p.website_url = Some("https://provider.example".into());
    assert!(p.validate().is_ok());
    p.website_url = Some("provider.example".into());
    assert!(matches!(
        p.validate(),
        Err(ValidationError::BadWebsiteUrl(_))
    ));
    p.website_url = None;
    assert!(p.validate().is_ok());
}

#[test]
fn notes_have_a_length_cap() {
    let mut p = profile(AppKind::Claude);
    p.notes = Some("短备注".into());
    assert!(p.validate().is_ok());
    p.notes = Some("长".repeat(MAX_NOTES_LEN + 1));
    assert!(matches!(
        p.validate(),
        Err(ValidationError::NotesTooLong(_))
    ));
}

#[test]
fn rejects_empty_api_key() {
    let mut p = profile(AppKind::Codex);
    p.api_key.clear();
    assert_eq!(p.validate(), Err(ValidationError::EmptyApiKey));
}

#[test]
fn accepts_api_keys_for_both_clients() {
    let mut codex = profile(AppKind::Codex);
    codex.api_key = "sk-live-codex".into();
    assert!(codex.validate().is_ok());
    let mut claude = profile(AppKind::Claude);
    claude.api_key = "sk-ant-claude".into();
    assert!(claude.validate().is_ok());
}

#[test]
fn protocol_owns_the_upstream_authentication_scheme() {
    assert_eq!(
        UpstreamProtocol::Responses.authentication_scheme(),
        crate::contracts::AuthenticationScheme::Bearer
    );
    assert_eq!(
        UpstreamProtocol::ChatCompletions.authentication_scheme(),
        crate::contracts::AuthenticationScheme::Bearer
    );
    assert_eq!(
        UpstreamProtocol::AnthropicMessages.authentication_scheme(),
        crate::contracts::AuthenticationScheme::XApiKey
    );
}

#[test]
fn codex_anthropic_routes_require_an_explicit_positive_output_limit() {
    let mut route = profile(AppKind::Codex);
    route.upstream_protocol = Some(UpstreamProtocol::AnthropicMessages);
    assert_eq!(
        route.validate(),
        Err(ValidationError::CodexAnthropicRequiresMaxOutputTokens)
    );
    route.max_output_tokens = Some(0).into();
    assert_eq!(
        route.validate(),
        Err(ValidationError::CodexAnthropicRequiresMaxOutputTokens)
    );
    route.max_output_tokens = Some(8_192).into();
    assert!(route.validate().is_ok());

    route.upstream_protocol = Some(UpstreamProtocol::ChatCompletions);
    assert_eq!(
        route.validate(),
        Err(ValidationError::UnexpectedMaxOutputTokens)
    );
}

#[test]
fn rejects_model_options_that_do_not_match_the_app() {
    let mut p = profile(AppKind::Codex);
    p.model_options = Some(ModelOptions::Claude(ClaudeModelSettings {
        primary_one_m: false,
        haiku_model: None,
        sonnet_model: None,
        sonnet_one_m: false,
        opus_model: None,
        opus_one_m: false,
        available_models: None,
    }));
    assert!(matches!(
        p.validate(),
        Err(ValidationError::ModelOptionsMismatch { .. })
    ));
}

#[test]
fn rejects_zero_context_window_and_blank_available_models() {
    let mut p = profile(AppKind::Codex);
    p.model_options = Some(ModelOptions::Codex(CodexModelSettings {
        context_window: Some(0),
    }));
    assert_eq!(p.validate(), Err(ValidationError::BadContextWindow));

    let mut c = profile(AppKind::Claude);
    c.model_options = Some(ModelOptions::Claude(ClaudeModelSettings {
        primary_one_m: false,
        haiku_model: None,
        sonnet_model: None,
        sonnet_one_m: false,
        opus_model: None,
        opus_one_m: false,
        available_models: Some(vec!["claude-opus-4".into(), "  ".into()]),
    }));
    assert_eq!(c.validate(), Err(ValidationError::EmptyAvailableModel));
}

#[test]
fn claude_one_m_is_explicit_and_model_identifiers_are_marker_free() {
    let mut primary = profile(AppKind::Claude);
    primary.model_options = Some(ModelOptions::Claude(ClaudeModelSettings {
        primary_one_m: true,
        haiku_model: None,
        sonnet_model: None,
        sonnet_one_m: false,
        opus_model: None,
        opus_one_m: false,
        available_models: None,
    }));
    assert!(primary.validate().is_ok());

    primary.model = Some("opus[1m]".into());
    assert_eq!(
        primary.validate(),
        Err(ValidationError::InlineOneMMarker { field: "主模型" })
    );

    primary.model = Some("opus[1M]".into());
    assert_eq!(
        primary.validate(),
        Err(ValidationError::InlineOneMMarker { field: "主模型" })
    );

    let mut missing_primary = profile(AppKind::Claude);
    missing_primary.model = None;
    missing_primary.model_options = Some(ModelOptions::Claude(ClaudeModelSettings {
        primary_one_m: true,
        haiku_model: None,
        sonnet_model: None,
        sonnet_one_m: false,
        opus_model: None,
        opus_one_m: false,
        available_models: None,
    }));
    assert_eq!(
        missing_primary.validate(),
        Err(ValidationError::OneMRequiresModel { field: "主模型" })
    );

    let mut mappings = profile(AppKind::Claude);
    mappings.model_options = Some(ModelOptions::Claude(ClaudeModelSettings {
        primary_one_m: false,
        haiku_model: Some("haiku[1m]".into()),
        sonnet_model: Some("sonnet".into()),
        sonnet_one_m: true,
        opus_model: Some("opus".into()),
        opus_one_m: true,
        available_models: Some(vec!["opus[1m]".into()]),
    }));
    assert_eq!(
        mappings.validate(),
        Err(ValidationError::InlineOneMMarker { field: "Haiku 档" })
    );

    mappings.model_options = Some(ModelOptions::Claude(ClaudeModelSettings {
        primary_one_m: false,
        haiku_model: None,
        sonnet_model: None,
        sonnet_one_m: true,
        opus_model: Some("opus".into()),
        opus_one_m: false,
        available_models: None,
    }));
    assert_eq!(
        mappings.validate(),
        Err(ValidationError::OneMRequiresModel {
            field: "Sonnet 档"
        })
    );
}

#[test]
fn default_settings_validate_and_reject_unknown_keys_loudly() {
    for app in [AppKind::Codex, AppKind::Claude] {
        assert!(default_common_settings(app).validate_for(app).is_ok());
    }

    let mut settings = default_common_settings(AppKind::Codex);
    settings.settings.insert(
        "threads".to_string(),
        CommonSettingValue::Explicit {
            value: ConfigValue::Bool(true),
        },
    );
    let error = settings.validate_for(AppKind::Codex).unwrap_err();
    assert!(matches!(error, ValidationError::UnknownCommonKey { .. }));
    assert!(error.to_string().contains("threads"));

    let mut provider_key = default_common_settings(AppKind::Claude);
    provider_key.settings.insert(
        "model".to_string(),
        CommonSettingValue::Explicit {
            value: ConfigValue::Str("m".into()),
        },
    );
    assert!(matches!(
        provider_key.validate_for(AppKind::Claude),
        Err(ValidationError::UnknownCommonKey { .. })
    ));
}

#[test]
fn incomplete_settings_are_rejected_with_the_missing_key() {
    let mut settings = default_common_settings(AppKind::Codex);
    settings.settings.remove("model_reasoning_effort");
    let error = settings.validate_for(AppKind::Codex).unwrap_err();
    assert_eq!(
        error,
        ValidationError::MissingCommonKey {
            key: "model_reasoning_effort".to_string()
        }
    );
}

#[test]
fn choice_values_must_be_catalog_values_and_toggles_must_be_bools() {
    let mut settings = default_common_settings(AppKind::Codex);
    settings.settings.insert(
        "model_reasoning_effort".to_string(),
        CommonSettingValue::Explicit {
            value: ConfigValue::Str("xhigh".into()),
        },
    );
    assert!(settings.validate_for(AppKind::Codex).is_ok());

    settings.settings.insert(
        "model_reasoning_effort".to_string(),
        CommonSettingValue::Explicit {
            value: ConfigValue::Str("extreme".into()),
        },
    );
    let error = settings.validate_for(AppKind::Codex).unwrap_err();
    assert!(matches!(error, ValidationError::BadCommonValue { .. }));
    assert!(error.to_string().contains("minimal"));

    settings.settings.insert(
        "model_reasoning_effort".to_string(),
        CommonSettingValue::Explicit {
            value: ConfigValue::Bool(true),
        },
    );
    assert!(matches!(
        settings.validate_for(AppKind::Codex),
        Err(ValidationError::BadCommonValue { .. })
    ));

    let mut toggled = default_common_settings(AppKind::Claude);
    toggled.settings.insert(
        "autoCompactEnabled".to_string(),
        CommonSettingValue::Explicit {
            value: ConfigValue::Str("on".into()),
        },
    );
    assert!(matches!(
        toggled.validate_for(AppKind::Claude),
        Err(ValidationError::BadCommonValue { .. })
    ));

    // Both polarities are legal: the parameter is a plain value.
    let mut both = default_common_settings(AppKind::Claude);
    both.settings.insert(
        "autoCompactEnabled".to_string(),
        CommonSettingValue::Explicit {
            value: ConfigValue::Bool(false),
        },
    );
    assert!(both.validate_for(AppKind::Claude).is_ok());
    both.settings.insert(
        "autoCompactEnabled".to_string(),
        CommonSettingValue::Explicit {
            value: ConfigValue::Bool(true),
        },
    );
    assert!(both.validate_for(AppKind::Claude).is_ok());
}

#[test]
fn plan_uses_the_profile_app_for_the_fixed_common_settings() {
    let claude_profile = profile(AppKind::Claude);
    assert!(validate_plan(&claude_profile, &default_common_settings(AppKind::Claude),).is_ok());
    assert!(matches!(
        validate_plan(&claude_profile, &default_common_settings(AppKind::Codex),),
        Err(ValidationError::UnknownCommonKey { .. })
    ));
}

#[test]
fn usage_query_contract_rejects_empty_paths_and_scripts_before_persistence() {
    let valid = UsageQuery::Declarative {
        url: "{{baseUrl}}/balance".to_string(),
        remaining_path: Some("data/balance".to_string()),
        used_path: None,
        total_path: None,
        unit: Some("USD".to_string()),
        refresh_interval_minutes: 0,
    };
    assert!(validate_usage_query(&valid).is_ok());

    let empty_paths = UsageQuery::Declarative {
        url: "https://relay.example/balance".to_string(),
        remaining_path: None,
        used_path: None,
        total_path: None,
        unit: None,
        refresh_interval_minutes: 0,
    };
    assert_eq!(
        validate_usage_query(&empty_paths),
        Err(ValidationError::UsageQueryExtractsNothing)
    );

    let blank_script = UsageQuery::Script {
        source: " \n".to_string(),
        refresh_interval_minutes: 0,
    };
    assert_eq!(
        validate_usage_query(&blank_script),
        Err(ValidationError::EmptyUsageQueryScript)
    );
}

#[test]
fn usage_query_contract_bounds_the_auto_refresh_interval() {
    let too_large = UsageQuery::Script {
        source: "({})".to_string(),
        refresh_interval_minutes: 1441,
    };
    assert_eq!(
        validate_usage_query(&too_large),
        Err(ValidationError::UsageQueryRefreshIntervalTooLarge(1440))
    );

    let at_bound = UsageQuery::Script {
        source: "({})".to_string(),
        refresh_interval_minutes: 1440,
    };
    assert!(validate_usage_query(&at_bound).is_ok());
}

fn official_profile(app: AppKind) -> ProviderProfile {
    let mut p = profile(app);
    p.route_mode = RouteMode::Official;
    p.model = None;
    p.base_url = None;
    p.api_key = String::new();
    p.upstream_protocol = None;
    p
}

#[test]
fn official_quota_interval_belongs_to_official_codex_only() {
    let mut codex = official_profile(AppKind::Codex);
    codex.official_quota_refresh_interval_minutes = Some(30);
    assert_eq!(codex.validate(), Ok(()));

    let mut claude = official_profile(AppKind::Claude);
    claude.official_quota_refresh_interval_minutes = Some(30);
    assert_eq!(
        claude.validate(),
        Err(ValidationError::QuotaIntervalRequiresOfficialCodex)
    );

    let mut custom = profile(AppKind::Codex);
    custom.official_quota_refresh_interval_minutes = Some(30);
    assert_eq!(
        custom.validate(),
        Err(ValidationError::QuotaIntervalRequiresOfficialCodex)
    );
}

#[test]
fn official_quota_interval_bounds_exclude_zero_without_a_second_off_shape() {
    let mut too_large = official_profile(AppKind::Codex);
    too_large.official_quota_refresh_interval_minutes = Some(1441);
    assert_eq!(
        too_large.validate(),
        Err(ValidationError::OfficialQuotaRefreshIntervalOutOfRange(
            1440
        ))
    );

    let mut zero = official_profile(AppKind::Codex);
    zero.official_quota_refresh_interval_minutes = Some(0);
    assert_eq!(
        zero.validate(),
        Err(ValidationError::OfficialQuotaRefreshIntervalOutOfRange(
            1440
        ))
    );

    let mut at_bound = official_profile(AppKind::Codex);
    at_bound.official_quota_refresh_interval_minutes = Some(1440);
    assert_eq!(at_bound.validate(), Ok(()));
}
