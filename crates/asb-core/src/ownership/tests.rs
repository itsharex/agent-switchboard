use super::*;

use crate::contracts::{AppKind, CommonSettingValue};

#[test]
fn every_catalog_key_is_common_owned_in_the_directory() {
    for app in [AppKind::Codex, AppKind::Claude] {
        for toggle in common_toggles(app) {
            assert!(
                is_owned(app, toggle.key),
                "{} toggle key must be app-owned",
                toggle.key
            );
            assert!(
                owner_for(app, toggle.key) == SettingOwner::Common,
                "{} toggle key must be common-owned",
                toggle.key
            );
        }
        for choice in common_choices(app) {
            assert!(
                is_owned(app, choice.key),
                "{} choice key must be app-owned",
                choice.key
            );
            assert!(
                owner_for(app, choice.key) == SettingOwner::Common,
                "{} choice key must be common-owned",
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
            assert_eq!(owner_for(app, entry.path), SettingOwner::Common);
        }
    }
}

#[test]
fn every_catalog_group_is_declared_and_non_empty() {
    for app in [AppKind::Codex, AppKind::Claude] {
        let groups = common_groups(app);
        for group in groups {
            let group = *group;
            let members = common_toggles(app)
                .iter()
                .filter(|t| t.group == group)
                .count()
                + common_choices(app)
                    .iter()
                    .filter(|c| c.group == group)
                    .count();
            assert!(members > 0, "分组 {group} 必须至少有一个选项");
        }
        for spec in common_toggles(app) {
            assert!(
                groups.contains(&spec.group),
                "开关 {} 的分组 {} 未在 common_groups 声明",
                spec.key,
                spec.group
            );
        }
        for spec in common_choices(app) {
            assert!(
                groups.contains(&spec.group),
                "档位 {} 的分组 {} 未在 common_groups 声明",
                spec.key,
                spec.group
            );
        }
    }
}

#[test]
fn automatic_common_settings_cover_exactly_the_catalog_keys() {
    for app in [AppKind::Codex, AppKind::Claude] {
        let defaults = default_common_settings(app);
        let catalog_keys: Vec<&str> = setting_specs(app)
            .into_iter()
            .filter(|spec| spec.owner == SettingOwner::Common)
            .map(|spec| spec.key)
            .collect();
        assert_eq!(defaults.settings.len(), catalog_keys.len());
        for key in catalog_keys {
            let value = defaults
                .value(key)
                .expect("every catalog key has an automatic value");
            assert!(matches!(value, CommonSettingValue::Automatic));
        }
    }
}

#[test]
fn codex_routing_keys_include_the_builtin_openai_override() {
    assert!(is_owned(AppKind::Codex, "model"));
    assert!(is_owned(AppKind::Codex, "model_provider"));
    assert!(is_owned(AppKind::Codex, "openai_base_url"));
    assert!(is_owned(AppKind::Codex, "model_reasoning_effort"));
    assert!(is_owned(AppKind::Codex, "model_context_window"));
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
