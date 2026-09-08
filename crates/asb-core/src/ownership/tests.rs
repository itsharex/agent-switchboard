use super::*;

use crate::contracts::{AppKind, SettingValue};

#[test]
fn every_editor_key_declares_its_directory_owner() {
    for app in [AppKind::Codex, AppKind::Claude] {
        for toggle in setting_toggles(app) {
            assert!(
                is_owned(app, toggle.key),
                "{} toggle key must be app-owned",
                toggle.key
            );
            assert!(
                owner_for(app, toggle.key) == toggle.owner,
                "{} toggle key must preserve its explicit owner",
                toggle.key
            );
        }
        for choice in setting_choices(app) {
            assert!(
                is_owned(app, choice.key),
                "{} choice key must be app-owned",
                choice.key
            );
            assert!(
                owner_for(app, choice.key) == choice.owner,
                "{} choice key must preserve its explicit owner",
                choice.key
            );
            assert!(
                !choice.options.is_empty(),
                "{} must offer at least one value",
                choice.key
            );
        }
    }
}

#[test]
fn official_directory_exposes_direct_and_preserved_boundaries() {
    for app in [AppKind::Codex, AppKind::Claude] {
        let directory = official_setting_directory(app);
        assert!(directory
            .iter()
            .any(|entry| { entry.disposition == OfficialSettingDisposition::Direct }));
        assert!(directory
            .iter()
            .any(|entry| { entry.disposition == OfficialSettingDisposition::PreserveOnly }));
        assert!(directory.iter().all(|entry| !entry.title.is_empty()
            && !entry.path.is_empty()
            && !entry.detail.is_empty()));
        for entry in directory
            .iter()
            .filter(|entry| entry.disposition == OfficialSettingDisposition::Direct)
        {
            assert_ne!(
                setting_spec(app, entry.path).unwrap().control,
                SettingControl::None
            );
        }
    }
}

#[test]
fn every_catalog_group_is_declared_and_non_empty() {
    for app in [AppKind::Codex, AppKind::Claude] {
        let groups = setting_groups(app);
        for group in groups {
            let group = *group;
            let members = setting_toggles(app)
                .iter()
                .filter(|t| t.group == group)
                .count()
                + setting_choices(app)
                    .iter()
                    .filter(|c| c.group == group)
                    .count();
            assert!(members > 0, "分组 {group} 必须至少有一个选项");
        }
        for spec in setting_toggles(app) {
            assert!(
                groups.contains(&spec.group),
                "开关 {} 的分组 {} 未在 setting_groups 声明",
                spec.key,
                spec.group
            );
        }
        for spec in setting_choices(app) {
            assert!(
                groups.contains(&spec.group),
                "档位 {} 的分组 {} 未在 setting_groups 声明",
                spec.key,
                spec.group
            );
        }
    }
}

#[test]
fn automatic_values_cover_exactly_each_ownership_scope() {
    for app in [AppKind::Codex, AppKind::Claude] {
        for (owner, defaults) in [
            (SettingOwner::Client, default_client_settings(app)),
            (SettingOwner::Provider, default_provider_parameters(app)),
        ] {
            let catalog_keys: Vec<&str> = setting_specs(app)
                .into_iter()
                .filter(|spec| spec.owner == owner && spec.control != SettingControl::None)
                .map(|spec| spec.key)
                .collect();
            assert_eq!(defaults.settings.len(), catalog_keys.len());
            for key in catalog_keys {
                assert!(matches!(defaults.value(key), Some(SettingValue::Automatic)));
            }
        }
    }
}

#[test]
fn provider_parameters_always_remove_prior_values_when_automatic() {
    for app in [AppKind::Codex, AppKind::Claude] {
        for key in default_provider_parameters(app).settings.keys() {
            assert_eq!(owner_for(app, key), SettingOwner::Provider);
            assert_eq!(
                provider_absent_action(app, key),
                Some(ProviderAbsentAction::Remove)
            );
        }
    }
    for key in [
        "features.fast_mode",
        "features.enable_request_compression",
        "features.personality",
    ] {
        assert_eq!(owner_for(AppKind::Codex, key), SettingOwner::Provider);
    }
    assert_eq!(
        owner_for(AppKind::Codex, "tui.animations"),
        SettingOwner::Client
    );
    assert_eq!(
        owner_for(AppKind::Codex, "sandbox_mode"),
        SettingOwner::Client
    );
}

#[test]
fn codex_routing_keys_own_custom_provider_capabilities_and_retired_override() {
    assert!(is_owned(AppKind::Codex, "model"));
    assert!(is_owned(AppKind::Codex, "model_provider"));
    assert!(is_owned(AppKind::Codex, "openai_base_url"));
    assert!(is_owned(AppKind::Codex, CODEX_PROVIDER_BASE_URL_KEY));
    assert!(!is_owned(AppKind::Codex, "model_providers.agent_switchboard.name"));
    assert!(!is_owned(AppKind::Codex, "model_providers.OpenAi.base_url"));
    assert!(is_owned(AppKind::Codex, "model_reasoning_effort"));
    assert!(is_owned(AppKind::Codex, "model_context_window"));
}

#[test]
fn codex_subagent_defaults_are_provider_owned_and_catalogued_for_their_controls() {
    let model = setting_spec(AppKind::Codex, CODEX_SUBAGENT_MODEL_KEY).expect("subagent model");
    assert_eq!(model.owner, SettingOwner::Provider);
    assert_eq!(model.control, SettingControl::ModelPicker);
    assert_eq!(model.group, Some("子 agent"));

    let reasoning = choice_spec(AppKind::Codex, CODEX_SUBAGENT_REASONING_EFFORT_KEY)
        .expect("subagent reasoning");
    assert_eq!(reasoning.owner, SettingOwner::Provider);
    assert_eq!(reasoning.group, "子 agent");
    assert_eq!(reasoning.control, ChoiceControl::Slider);

    let defaults = default_provider_parameters(AppKind::Codex);
    assert_eq!(
        defaults.value(CODEX_SUBAGENT_MODEL_KEY),
        Some(&SettingValue::Automatic)
    );
    assert_eq!(
        defaults.value(CODEX_SUBAGENT_REASONING_EFFORT_KEY),
        Some(&SettingValue::Automatic)
    );
}

#[test]
fn claude_model_tiers_and_available_models_are_owned() {
    assert!(is_owned(
        AppKind::Claude,
        "env.ANTHROPIC_DEFAULT_HAIKU_MODEL"
    ));
    assert!(is_owned(
        AppKind::Claude,
        "env.ANTHROPIC_DEFAULT_SONNET_MODEL"
    ));
    assert!(is_owned(
        AppKind::Claude,
        "env.ANTHROPIC_DEFAULT_OPUS_MODEL"
    ));
    assert!(is_owned(AppKind::Claude, "availableModels"));
}

#[test]
fn claude_ultracode_is_an_independent_setting_not_an_effort_level() {
    let effort = choice_spec(AppKind::Claude, "effortLevel").expect("effort level");
    assert!(matches!(effort.control, ChoiceControl::Slider));
    assert!(effort
        .options
        .iter()
        .all(|option| option.value != "ultracode"));
    let ultracode = toggle_spec(AppKind::Claude, "ultracode").expect("ultracode toggle");
    assert_eq!(ultracode.group, "模型行为");
}

#[test]
fn deprecated_claude_model_key_is_provider_owned_and_removed_when_absent() {
    assert_eq!(
        owner_for(AppKind::Claude, "env.ANTHROPIC_SMALL_FAST_MODEL"),
        SettingOwner::Provider
    );
    assert_eq!(
        provider_absent_action(AppKind::Claude, "env.ANTHROPIC_SMALL_FAST_MODEL"),
        Some(ProviderAbsentAction::Remove)
    );
}

#[test]
fn host_keys_are_not_owned() {
    assert!(!is_owned(AppKind::Codex, "threads"));
    assert!(!is_owned(AppKind::Codex, "model_providers.openai.base_url"));
    assert!(!is_owned(AppKind::Claude, "permissions"));
    assert!(!is_owned(AppKind::Claude, "env.HTTP_PROXY"));
}

#[test]
fn provider_keys_expose_types_and_a_cleanup_action_from_the_one_directory() {
    let model = setting_spec(AppKind::Codex, "model").expect("model spec");
    assert_eq!(model.owner, SettingOwner::Provider);
    assert_eq!(model.value_type, SettingValueType::String);
    assert_eq!(
        model.provider_absent_action,
        Some(ProviderAbsentAction::Remove)
    );
    assert!(is_provider_owned(AppKind::Codex, "openai_base_url"));
    assert_eq!(owner_for(AppKind::Codex, "threads"), SettingOwner::Host);
}
