use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize,
};

/// A nullable persisted value whose field itself is mandatory. This prevents
/// old provider files from being accepted merely because an optional Rust
/// field silently deserialized as `None` when absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ExplicitMaxOutputTokens(Option<u64>);

impl ExplicitMaxOutputTokens {
    pub const fn none() -> Self {
        Self(None)
    }

    pub const fn some(value: u64) -> Self {
        Self(Some(value))
    }

    pub const fn value(self) -> Option<u64> {
        self.0
    }

    pub const fn is_some(self) -> bool {
        self.0.is_some()
    }
}

impl From<Option<u64>> for ExplicitMaxOutputTokens {
    fn from(value: Option<u64>) -> Self {
        Self(value)
    }
}

impl<'de> Deserialize<'de> for ExplicitMaxOutputTokens {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ExplicitMaxOutputTokensVisitor;

        impl<'de> Visitor<'de> for ExplicitMaxOutputTokensVisitor {
            type Value = ExplicitMaxOutputTokens;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a non-negative integer or null")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ExplicitMaxOutputTokens::none())
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ExplicitMaxOutputTokens::none())
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ExplicitMaxOutputTokens::some(value))
            }
        }

        // `deserialize_any` makes a missing struct field fail before it can
        // be mistaken for JSON `null`, while preserving `null` as the
        // explicit "not applicable" value in persisted provider files.
        deserializer.deserialize_any(ExplicitMaxOutputTokensVisitor)
    }
}

/// Codex model run parameters owned by one profile. An absent context window
/// removes the managed line on switch, so one provider cannot leak it into
/// the next. Reasoning effort, summary, and verbosity are general settings
/// owned by the settings page, not profile fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexModelSettings {
    /// Optional model context window in tokens.
    pub context_window: Option<u64>,
}

/// Claude Code model mapping owned by one profile. The primary model is the
/// profile's `model` field; these are the remaining tiers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeModelSettings {
    /// The primary model itself lives on `ProviderProfile::model`.
    pub primary_one_m: bool,
    pub haiku_model: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub haiku_one_m: bool,
    pub sonnet_model: Option<String>,
    pub sonnet_one_m: bool,
    pub opus_model: Option<String>,
    pub opus_one_m: bool,
    /// Optional `availableModels` list in settings.json.
    pub available_models: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fable_model: Option<String>,
    #[serde(default)]
    pub fable_one_m: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_model: Option<String>,
    #[serde(default)]
    pub subagent_one_m: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_names: Option<ClaudeModelDisplayNames>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeModelDisplayNames {
    pub haiku: Option<String>,
    pub sonnet: Option<String>,
    pub opus: Option<String>,
    pub fable: Option<String>,
}

/// Per-client model options attached to a profile. The variant must match the
/// profile's app; validation rejects mismatches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ModelOptions {
    Codex(CodexModelSettings),
    Claude(ClaudeModelSettings),
}

fn is_false(value: &bool) -> bool {
    !*value
}
