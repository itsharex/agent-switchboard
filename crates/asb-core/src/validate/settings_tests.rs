use super::tests::profile;
use super::*;
use crate::contracts::{AppKind, ConfigValue, SettingValue};
use crate::ownership::{default_client_settings, default_provider_parameters};

#[test]
fn default_settings_validate_and_reject_unknown_keys_loudly() {
    for app in [AppKind::Codex, AppKind::Claude] {
        assert!(default_client_settings(app)
            .validate_client_settings(app)
            .is_ok());
        assert!(default_provider_parameters(app)
            .validate_provider_parameters(app)
            .is_ok());
    }

    let mut settings = default_client_settings(AppKind::Codex);
    settings.settings.insert(
        "threads".to_string(),
        SettingValue::Explicit {
            value: ConfigValue::Bool(true),
        },
    );
    let error = settings
        .validate_client_settings(AppKind::Codex)
        .unwrap_err();
    assert!(matches!(error, ValidationError::UnknownSettingKey { .. }));
    assert!(error.to_string().contains("threads"));

    let mut provider_key = default_client_settings(AppKind::Claude);
    provider_key.settings.insert(
        "model".to_string(),
        SettingValue::Explicit {
            value: ConfigValue::Str("m".into()),
        },
    );
    assert!(matches!(
        provider_key.validate_client_settings(AppKind::Claude),
        Err(ValidationError::UnknownSettingKey { .. })
    ));
}

#[test]
fn settings_reject_keys_from_the_other_scope_and_missing_client_preferences() {
    let mut client = default_client_settings(AppKind::Codex);
    client
        .settings
        .insert("model_reasoning_effort".into(), SettingValue::Automatic);
    assert!(matches!(client.validate_client_settings(AppKind::Codex),
        Err(ValidationError::UnknownSettingKey { key, .. }) if key == "model_reasoning_effort"));

    let mut parameters = default_provider_parameters(AppKind::Codex);
    parameters
        .settings
        .insert("sandbox_mode".into(), SettingValue::Automatic);
    assert!(
        matches!(parameters.validate_provider_parameters(AppKind::Codex),
        Err(ValidationError::UnknownSettingKey { key, .. }) if key == "sandbox_mode")
    );

    let mut incomplete = default_client_settings(AppKind::Claude);
    incomplete.settings.remove("spinnerTipsEnabled");
    assert_eq!(
        incomplete.validate_client_settings(AppKind::Claude),
        Err(ValidationError::MissingSettingKey {
            key: "spinnerTipsEnabled".into()
        })
    );
}

#[test]
fn profile_and_plan_validation_require_complete_provider_parameters() {
    let mut profile = profile(AppKind::Codex);
    profile.parameters.settings.remove("web_search");
    let expected = Err(ValidationError::MissingSettingKey {
        key: "web_search".into(),
    });
    assert_eq!(profile.validate(), expected);
    assert_eq!(
        validate_plan(&profile, &default_client_settings(AppKind::Codex)),
        expected
    );
}

#[test]
fn incomplete_settings_are_rejected_with_the_missing_key() {
    let mut settings = default_provider_parameters(AppKind::Codex);
    settings.settings.remove("model_reasoning_effort");
    let error = settings
        .validate_provider_parameters(AppKind::Codex)
        .unwrap_err();
    assert_eq!(
        error,
        ValidationError::MissingSettingKey {
            key: "model_reasoning_effort".to_string()
        }
    );
}

#[test]
fn choice_values_must_be_catalog_values_and_toggles_must_be_bools() {
    let mut settings = default_provider_parameters(AppKind::Codex);
    settings.settings.insert(
        "model_reasoning_effort".to_string(),
        SettingValue::Explicit {
            value: ConfigValue::Str("xhigh".into()),
        },
    );
    assert!(settings
        .validate_provider_parameters(AppKind::Codex)
        .is_ok());

    settings.settings.insert(
        "model_reasoning_effort".to_string(),
        SettingValue::Explicit {
            value: ConfigValue::Str("extreme".into()),
        },
    );
    let error = settings
        .validate_provider_parameters(AppKind::Codex)
        .unwrap_err();
    assert!(matches!(error, ValidationError::BadSettingValue { .. }));
    assert!(error.to_string().contains("minimal"));

    settings.settings.insert(
        "model_reasoning_effort".to_string(),
        SettingValue::Explicit {
            value: ConfigValue::Bool(true),
        },
    );
    assert!(matches!(
        settings.validate_provider_parameters(AppKind::Codex),
        Err(ValidationError::BadSettingValue { .. })
    ));

    let mut toggled = default_provider_parameters(AppKind::Claude);
    toggled.settings.insert(
        "autoCompactEnabled".to_string(),
        SettingValue::Explicit {
            value: ConfigValue::Str("on".into()),
        },
    );
    assert!(matches!(
        toggled.validate_provider_parameters(AppKind::Claude),
        Err(ValidationError::BadSettingValue { .. })
    ));

    // Both polarities are legal: the parameter is a plain value.
    let mut both = default_provider_parameters(AppKind::Claude);
    both.settings.insert(
        "autoCompactEnabled".to_string(),
        SettingValue::Explicit {
            value: ConfigValue::Bool(false),
        },
    );
    assert!(both.validate_provider_parameters(AppKind::Claude).is_ok());
    both.settings.insert(
        "autoCompactEnabled".to_string(),
        SettingValue::Explicit {
            value: ConfigValue::Bool(true),
        },
    );
    assert!(both.validate_provider_parameters(AppKind::Claude).is_ok());
}

#[test]
fn provider_owned_subagent_model_requires_a_nonempty_model_identifier() {
    let mut settings = default_provider_parameters(AppKind::Codex);
    settings.settings.insert(
        "agents.default_subagent_model".to_string(),
        SettingValue::Explicit {
            value: ConfigValue::Str("relay-coder".into()),
        },
    );
    assert!(settings
        .validate_provider_parameters(AppKind::Codex)
        .is_ok());

    settings.settings.insert(
        "agents.default_subagent_model".to_string(),
        SettingValue::Explicit {
            value: ConfigValue::Str("   ".into()),
        },
    );
    let error = settings
        .validate_provider_parameters(AppKind::Codex)
        .expect_err("blank model ids must not be projected");
    assert!(matches!(error, ValidationError::BadSettingValue { .. }));
    assert!(error.to_string().contains("非空模型标识"));
}

#[test]
fn plan_uses_the_profile_app_for_the_fixed_settings() {
    let claude_profile = profile(AppKind::Claude);
    assert!(validate_plan(&claude_profile, &default_client_settings(AppKind::Claude),).is_ok());
    assert!(matches!(
        validate_plan(&claude_profile, &default_client_settings(AppKind::Codex),),
        Err(ValidationError::UnknownSettingKey { .. })
    ));
}
