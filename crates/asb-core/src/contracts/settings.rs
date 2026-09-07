use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::contracts::AppKind;

/// A plain configuration value. The array shape exists for provider-owned
/// lists such as `availableModels` and can never pass common-settings
/// validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigValue {
    Bool(bool),
    Str(String),
    Number(f64),
    Array(Vec<ConfigValue>),
}

impl ConfigValue {
    /// Rendering for previews and diagnostics; secret-shaped keys are
    /// redacted by the caller through [`crate::redact`].
    pub fn display(&self) -> String {
        match self {
            ConfigValue::Bool(b) => b.to_string(),
            ConfigValue::Str(s) => s.clone(),
            ConfigValue::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            ConfigValue::Array(items) => {
                let parts: Vec<String> = items.iter().map(ConfigValue::display).collect();
                format!("[{}]", parts.join(", "))
            }
        }
    }
}

/// The application-owned intent for one officially supported common setting.
///
/// `Automatic` deliberately means that Agent Switchboard does not emit the
/// setting into the client configuration. It is not a synthetic host value:
/// the client and active model choose their own documented default. `Explicit`
/// writes exactly the value the user selected, including a value that happens
/// to match a documented default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum CommonSettingValue {
    Automatic,
    Explicit { value: ConfigValue },
}

/// The complete common-setting intent for exactly one client, persisted as
/// `common/{client}.json`. Every supported parameter has one tagged intent so
/// an absent client-file key can never be confused with an explicit false or
/// a guessed application default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommonSettings {
    pub settings: BTreeMap<String, CommonSettingValue>,
}

impl CommonSettings {
    pub fn value(&self, key: &str) -> Option<&CommonSettingValue> {
        self.settings.get(key)
    }
}

/// One client's common settings together with the stable revision used for
/// optimistic editor saves. It represents application state only and never a
/// client file preview or write.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommonSettingsSnapshot {
    pub settings: CommonSettings,
    pub settings_hash: String,
}

/// A side-effect-free rendering of the current draft's general-settings
/// fragment. It never includes provider routing, credentials, or host-owned
/// configuration.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommonSettingsPreview {
    pub app: AppKind,
    pub target: String,
    pub content: String,
}
