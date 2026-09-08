//! Codex-global sub-agent runtime controls: one typed resource, three scalars.
//!
//! This contract is deliberately separate from [`crate::contracts::SettingsValues`].
//! General client preferences are stored in application data first and only
//! later projected by a supplier switch. Runtime controls are read from and
//! written to the user-level `config.toml` directly; model and reasoning
//! defaults are provider parameters and never enter this resource.

use serde::{Deserialize, Serialize};

use crate::contracts::{AppKind, SettingValue};

/// The one canonical key inside `[agents]` managed by this module, in the
/// order the settings panel presents them. `AsRef<str>` yields the exact
/// TOML key; nothing else in the binary may redefine these names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexSubagentKey {
    Enabled,
    MaxConcurrentThreadsPerSession,
    InterruptMessage,
}

impl CodexSubagentKey {
    /// Every managed key, in display order.
    pub const ALL: [CodexSubagentKey; 3] = [
        CodexSubagentKey::Enabled,
        CodexSubagentKey::MaxConcurrentThreadsPerSession,
        CodexSubagentKey::InterruptMessage,
    ];

    /// The canonical TOML key under `[agents]`.
    pub fn as_str(self) -> &'static str {
        match self {
            CodexSubagentKey::Enabled => "enabled",
            CodexSubagentKey::MaxConcurrentThreadsPerSession => {
                "max_concurrent_threads_per_session"
            }
            CodexSubagentKey::InterruptMessage => "interrupt_message",
        }
    }

    /// The dotted path inside a Codex `config.toml`.
    pub fn path(self) -> String {
        format!("agents.{}", self.as_str())
    }
}

/// The three Codex-global sub-agent controls. Every field is an automatic-or-explicit
/// intent: `Automatic` means Agent Switchboard does not write that key at all,
/// so Codex applies its own documented default or resolution rule.
///
/// Deserialization rejects unknown members, so no retired alias such as
/// `allow_interrupt`, `max_concurrent_agents`, or `max_threads` can reach this
/// contract through any transport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexSubagentSettings {
    pub enabled: SettingValue,
    pub max_concurrent_threads_per_session: SettingValue,
    pub interrupt_message: SettingValue,
}

impl CodexSubagentSettings {
    /// Every field automatic: no managed key is written.
    pub fn automatic() -> Self {
        Self {
            enabled: SettingValue::Automatic,
            max_concurrent_threads_per_session: SettingValue::Automatic,
            interrupt_message: SettingValue::Automatic,
        }
    }

    pub fn get(&self, key: CodexSubagentKey) -> &SettingValue {
        match key {
            CodexSubagentKey::Enabled => &self.enabled,
            CodexSubagentKey::MaxConcurrentThreadsPerSession => {
                &self.max_concurrent_threads_per_session
            }
            CodexSubagentKey::InterruptMessage => &self.interrupt_message,
        }
    }
}

impl Default for CodexSubagentSettings {
    fn default() -> Self {
        Self::automatic()
    }
}

/// The three canonical runtime keys as read from the real user-level configuration,
/// plus the baseline hash every later write must still match.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexSubagentSettingsSnapshot {
    pub app: AppKind,
    pub settings: CodexSubagentSettings,
    /// SHA-256 of the exact user-level `config.toml` text this snapshot was
    /// read from; an empty file and a missing file both hash as empty but
    /// differ in `file_exists`.
    pub config_hash: String,
    pub file_exists: bool,
    /// Retired concurrency keys still present in the host file. They are never
    /// read, mapped, or converted; an explicit concurrency value is refused
    /// until the user removes them by hand.
    pub deprecated_keys: Vec<String>,
}

/// A side-effect-free rendering of the complete candidate file with secrets
/// removed. It shows the whole document, not just the managed keys, because
/// the write replaces the file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexSubagentSettingsPreview {
    pub app: AppKind,
    pub target: String,
    pub content: String,
    /// Hash of the current file the candidate was rendered against.
    pub config_hash: String,
    /// Hash of the exact, unredacted candidate. Application refuses a plan
    /// whose candidate no longer matches the previewed text.
    pub rendered_hash: String,
}

/// One confirmed sub-agent write. This is the only transaction shape this
/// module accepts: it carries no generic patch, no target path, and no
/// provider reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubagentSettingsPlan {
    pub settings: CodexSubagentSettings,
    /// Hash of the file as it was when the preview was produced.
    pub expected_hash: String,
    pub expected_target_existed: bool,
    /// Hash of the exact candidate the user confirmed.
    pub rendered_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::ConfigValue;

    #[test]
    fn automatic_defaults_write_nothing() {
        let settings = CodexSubagentSettings::automatic();
        for key in CodexSubagentKey::ALL {
            assert_eq!(settings.get(key), &SettingValue::Automatic);
        }
    }

    #[test]
    fn canonical_keys_match_the_official_reference() {
        let paths: Vec<String> = CodexSubagentKey::ALL.iter().map(|key| key.path()).collect();
        assert_eq!(
            paths,
            vec![
                "agents.enabled",
                "agents.max_concurrent_threads_per_session",
                "agents.interrupt_message",
            ]
        );
    }

    #[test]
    fn unknown_members_are_rejected() {
        let error = serde_json::from_str::<CodexSubagentSettings>(
            r#"{"enabled":{"mode":"automatic"},"maxConcurrentThreadsPerSession":{"mode":"automatic"},
                "interruptMessage":{"mode":"automatic"},"allowInterrupt":{"mode":"automatic"}}"#,
        )
        .expect_err("retired alias must not deserialize");
        assert!(error.to_string().contains("allowInterrupt"));
    }

    #[test]
    fn retired_alias_names_are_never_accepted_as_field_names() {
        for alias in ["allow_interrupt", "max_concurrent_agents", "max_threads"] {
            let payload = format!(
                r#"{{"{alias}":{{"mode":"explicit","value":true}},"enabled":{{"mode":"automatic"}},
                    "maxConcurrentThreadsPerSession":{{"mode":"automatic"}},
                    "interruptMessage":{{"mode":"automatic"}}}}"#
            );
            assert!(
                serde_json::from_str::<CodexSubagentSettings>(&payload).is_err(),
                "{alias} must be rejected"
            );
        }
    }

    #[test]
    fn a_complete_explicit_set_round_trips() {
        let settings = CodexSubagentSettings {
            enabled: SettingValue::Explicit {
                value: ConfigValue::Bool(true),
            },
            max_concurrent_threads_per_session: SettingValue::Explicit {
                value: ConfigValue::Number(3.0),
            },
            interrupt_message: SettingValue::Explicit {
                value: ConfigValue::Bool(false),
            },
        };
        let json = serde_json::to_string(&settings).expect("serialize");
        assert_eq!(
            serde_json::from_str::<CodexSubagentSettings>(&json).expect("deserialize"),
            settings
        );
    }

    #[test]
    fn plans_reject_unknown_members() {
        let plan = SubagentSettingsPlan {
            settings: CodexSubagentSettings::automatic(),
            expected_hash: "hash".to_string(),
            expected_target_existed: true,
            rendered_hash: "rendered".to_string(),
        };
        let json = serde_json::to_string(&plan).expect("serialize");
        assert_eq!(
            serde_json::from_str::<SubagentSettingsPlan>(&json).expect("deserialize"),
            plan
        );
        assert!(serde_json::from_str::<SubagentSettingsPlan>(
            r#"{"settings":null,"expectedHash":"h","expectedTargetExisted":true,"renderedHash":"r","extra":1}"#
        )
        .is_err());
    }
}
