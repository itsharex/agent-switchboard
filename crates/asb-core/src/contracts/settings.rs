use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::contracts::AppKind;

/// Largest integer that can round-trip through JSON's IEEE-754 number type
/// without a loss of precision. Typed configuration values use `f64`, so a
/// TOML integer outside this range must never be read into or written from a
/// `ConfigValue::Number`.
pub const MAX_EXACT_CONFIG_INTEGER: i64 = 9_007_199_254_740_991;

/// A plain configuration value. The array shape exists for provider-owned
/// lists such as `availableModels` and can never pass scalar-setting
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
                if n.fract() == 0.0 && n.abs() <= MAX_EXACT_CONFIG_INTEGER as f64 {
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

/// The stored intent for one officially supported scalar setting.
///
/// `Automatic` deliberately means that Agent Switchboard does not emit the
/// setting into the client configuration. It is not a synthetic host value:
/// the client and active model choose their own documented default. `Explicit`
/// writes exactly the value the user selected, including a value that happens
/// to match a documented default.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum SettingValue {
    Automatic,
    Explicit { value: ConfigValue },
}

impl<'de> Deserialize<'de> for SettingValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Serde ignores extra members on internally tagged unit variants.
        // An empty struct variant preserves the exact automatic wire shape.
        #[derive(Deserialize)]
        #[serde(tag = "mode", rename_all = "camelCase", deny_unknown_fields)]
        enum Intent {
            Automatic {},
            Explicit { value: ConfigValue },
        }
        Ok(match Intent::deserialize(deserializer)? {
            Intent::Automatic {} => Self::Automatic,
            Intent::Explicit { value } => Self::Explicit { value },
        })
    }
}

/// Complete scalar intents for a single ownership scope. Provider parameters
/// live in their provider profile; client settings live in their client file.
/// Each scope validates its exact key set against the ownership directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsValues {
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub claude_extra: crate::claude_common::Extra,
    pub settings: BTreeMap<String, SettingValue>,
}

impl SettingsValues {
    pub fn value(&self, key: &str) -> Option<&SettingValue> {
        self.settings.get(key)
    }
}

/// One client's settings together with the stable revision used for
/// optimistic editor saves. It represents application state only and never a
/// client file preview or write.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSettingsSnapshot {
    pub settings: SettingsValues,
    pub settings_hash: String,
}

/// A side-effect-free rendering of the current draft's client-settings
/// fragment. It never includes provider routing, credentials, or host-owned
/// configuration.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSettingsPreview {
    pub app: AppKind,
    pub target: String,
    pub content: String,
}
