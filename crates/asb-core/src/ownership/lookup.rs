use std::collections::BTreeMap;

use crate::contracts::{AppKind, CommonSettingValue, CommonSettings};
use crate::ownership::directory::{CLAUDE_DIRECTORY_FAMILIES, CODEX_DIRECTORY_FAMILIES};
use crate::ownership::provider::PROVIDER_SETTINGS;
use crate::ownership::spec::{
    ChoiceSpec, OfficialSettingDisposition, OfficialSettingEntry, ProviderAbsentAction,
    SettingControl, SettingOwner, SettingSpec, SettingValueType, ToggleSpec,
};
use crate::ownership::{CLAUDE_CHOICES, CLAUDE_TOGGLES, CODEX_CHOICES, CODEX_TOGGLES};

/// The common toggle entries for one client. They are a projection of the
/// ownership directory, not a second owned-key list.
pub fn common_toggles(app: AppKind) -> &'static [ToggleSpec] {
    match app {
        AppKind::Codex => CODEX_TOGGLES,
        AppKind::Claude => CLAUDE_TOGGLES,
    }
}

/// The common choice entries for one client. Values are the authority the
/// common-settings validator checks string entries against.
pub fn common_choices(app: AppKind) -> &'static [ChoiceSpec] {
    match app {
        AppKind::Codex => CODEX_CHOICES,
        AppKind::Claude => CLAUDE_CHOICES,
    }
}

/// Section order on the settings page, the single owner of grouping.
pub fn common_groups(app: AppKind) -> &'static [&'static str] {
    match app {
        AppKind::Codex => &[
            "模型行为",
            "安全与审批",
            "隐私与数据",
            "终端界面",
            "工具与功能",
        ],
        AppKind::Claude => &["模型行为", "界面与交互", "文件与 Git"],
    }
}

/// Complete official-settings coverage map for one client.
///
/// Direct parameter entries are derived from the same ownership directory
/// that validates and projects them. Structured, project-scoped and managed
/// families are listed alongside them with their truthful boundary, so the UI
/// never implies that a preserved resource is an editable scalar preference.
pub fn official_setting_directory(app: AppKind) -> Vec<OfficialSettingEntry> {
    let mut entries: Vec<OfficialSettingEntry> = setting_specs(app)
        .into_iter()
        .filter(|spec| spec.owner == SettingOwner::Common)
        .map(|spec| OfficialSettingEntry {
            title: spec
                .label
                .expect("common setting must have a visible label"),
            path: spec.key,
            related_paths: &[],
            disposition: OfficialSettingDisposition::Direct,
            detail: "通过“基础参数”编辑，保存后随供应商重新应用写入用户级配置。",
        })
        .collect();
    entries.extend_from_slice(match app {
        AppKind::Codex => CODEX_DIRECTORY_FAMILIES,
        AppKind::Claude => CLAUDE_DIRECTORY_FAMILIES,
    });
    entries
}

/// The catalog spec for a choice key, if the key is one.
pub fn choice_spec(app: AppKind, key: &str) -> Option<&'static ChoiceSpec> {
    common_choices(app).iter().find(|spec| spec.key == key)
}

/// The catalog spec for a toggle key, if the key is one.
pub fn toggle_spec(app: AppKind, key: &str) -> Option<&'static ToggleSpec> {
    common_toggles(app).iter().find(|spec| spec.key == key)
}

/// Returns every managed spec for one client. This is the single typed
/// directory exposed to adapters, validation, and the editor. Host keys do
/// not appear because the host namespace is intentionally open-ended.
pub fn setting_specs(app: AppKind) -> Vec<SettingSpec> {
    let mut specs = Vec::new();
    specs.extend(
        PROVIDER_SETTINGS
            .iter()
            .filter(|spec| spec.app == app)
            .map(|spec| SettingSpec {
                app,
                key: spec.key,
                owner: SettingOwner::Provider,
                value_type: spec.value_type,
                allowed_values: &[],
                control: SettingControl::None,
                label: None,
                group: None,
                provider_absent_action: Some(ProviderAbsentAction::Remove),
            }),
    );
    specs.extend(common_toggles(app).iter().map(|spec| SettingSpec {
        app,
        key: spec.key,
        owner: SettingOwner::Common,
        value_type: SettingValueType::Bool,
        allowed_values: &[],
        control: SettingControl::Toggle,
        label: Some(spec.label),
        group: Some(spec.group),
        provider_absent_action: None,
    }));
    specs.extend(common_choices(app).iter().map(|spec| SettingSpec {
        app,
        key: spec.key,
        owner: SettingOwner::Common,
        value_type: SettingValueType::String,
        allowed_values: spec.options,
        control: SettingControl::Choice {
            presentation: spec.control,
        },
        label: Some(spec.label),
        group: Some(spec.group),
        provider_absent_action: None,
    }));
    specs
}

/// Looks up one client configuration key in the ownership directory. An
/// unlisted key is explicitly host-owned, rather than being an implicit
/// application fallback.
pub fn setting_spec(app: AppKind, key: &str) -> Option<SettingSpec> {
    setting_specs(app).into_iter().find(|spec| spec.key == key)
}

/// Returns the complete ownership decision for a key. This is a lightweight
/// form for callers that do not need editor metadata.
pub fn owner_for(app: AppKind, key: &str) -> SettingOwner {
    setting_spec(app, key)
        .map(|spec| spec.owner)
        .unwrap_or(SettingOwner::Host)
}

/// Returns the provider cleanup action for a provider-owned key, if any.
pub fn provider_absent_action(app: AppKind, key: &str) -> Option<ProviderAbsentAction> {
    setting_spec(app, key)
        .filter(|spec| spec.owner == SettingOwner::Provider)
        .and_then(|spec| spec.provider_absent_action)
}

/// Compatibility-free semantic spelling for adapter collectors: a key is
/// managed when the directory says it is common or provider-owned.
pub fn is_owned(app: AppKind, key: &str) -> bool {
    owner_for(app, key) != SettingOwner::Host
}

/// Whether a key belongs to the selected provider profile rather than the
/// common settings. The client argument is required: the same spelling can
/// have different ownership in different configuration formats.
pub fn is_provider_owned(app: AppKind, key: &str) -> bool {
    owner_for(app, key) == SettingOwner::Provider
}

/// The complete automatic common-settings intent for one client, built from
/// this directory. A fresh installation and every "恢复默认值" action leave
/// each client key to the host and active model rather than writing a guessed
/// value into the real configuration file.
pub fn default_common_settings(app: AppKind) -> CommonSettings {
    let mut settings = BTreeMap::new();
    for spec in setting_specs(app)
        .into_iter()
        .filter(|spec| spec.owner == SettingOwner::Common)
    {
        settings.insert(spec.key.to_string(), CommonSettingValue::Automatic);
    }
    CommonSettings { settings }
}
