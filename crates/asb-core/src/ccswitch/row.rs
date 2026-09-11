use serde::{Deserialize, Serialize};

use crate::contracts::{
    AppKind, CodexEndpoint, CodexReasoningLevel, CodexUpstream, ProviderDraft,
    ResponsesRequestMode, SettingsValues, UsageQuery,
};

/// One source `providers` row, already read by the caller.
#[derive(Debug, Clone)]
pub struct CcSwitchRow {
    pub id: String,
    pub app_type: String,
    pub name: String,
    pub settings_config: String,
    /// Local metadata carried over on import; both columns are nullable.
    pub website_url: Option<String>,
    pub notes: Option<String>,
    /// Source-provider metadata. Only the optional usage script is
    /// considered; its source never crosses the scan-response boundary.
    pub meta: Option<String>,
}

/// An importable provider. `key` identifies the source
/// row (`"<app_type>:<id>"`) so the import command can re-resolve it against
/// a fresh scan.
#[derive(Debug, Clone, PartialEq)]
pub struct CcSwitchProposal {
    pub key: String,
    pub draft: CcSwitchProviderDraft,
    /// Field names that exist in the source but are not carried over. Never
    /// contains secret values, only key names.
    pub warnings: Vec<String>,
}

/// A source proposal keeps the two current destination contracts distinct.
/// Codex cannot be represented by the retired generic provider schema.
#[derive(Debug, Clone, PartialEq)]
pub enum CcSwitchProviderDraft {
    Codex(CodexImportSeed),
    Claude(ProviderDraft),
}

impl CcSwitchProviderDraft {
    pub const fn app(&self) -> AppKind {
        match self {
            Self::Codex(_) => AppKind::Codex,
            Self::Claude(_) => AppKind::Claude,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Codex(seed) => &seed.name,
            Self::Claude(draft) => &draft.name,
        }
    }
}

/// Source-owned Codex import facts for one imported row. The model catalog
/// and capability statements the strict profile contract requires are
/// completed and confirmed in the editor; a seed never persists directly.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexImportSeed {
    pub name: String,
    pub endpoint: CodexEndpoint,
    /// Empty when the source row exposes no usable credential; the editor
    /// blocks saving until the user provides one.
    pub api_key: String,
    pub upstream: CodexUpstream,
    pub request_mode: ResponsesRequestMode,
    pub default_model: String,
    pub catalog: Vec<CodexCatalogSeed>,
    pub parameters: SettingsValues,
    pub notes: Option<String>,
    pub website_url: Option<String>,
    pub usage_query: Option<UsageQuery>,
    /// Field names that exist in the source but are not carried over.
    pub warnings: Vec<String>,
}

impl std::fmt::Debug for CodexImportSeed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CodexImportSeed")
            .field("name", &self.name)
            .field("endpoint", &self.endpoint)
            .field("api_key", &crate::redact::REDACTED)
            .field("upstream", &self.upstream)
            .field("default_model", &self.default_model)
            .field("catalog", &self.catalog)
            .finish_non_exhaustive()
    }
}

/// One real model fact from the source catalog. Absent facts stay absent;
/// the editor supplies editable defaults and the user confirms them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexCatalogSeed {
    pub model: String,
    pub context_window: Option<u64>,
    pub images: Option<bool>,
    pub default_reasoning_level: Option<CodexReasoningLevel>,
    pub reasoning_levels: Option<Vec<CodexReasoningLevel>>,
}

/// A provider that cannot be imported, with a user-facing reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchSkip {
    pub key: String,
    pub app_type: String,
    pub name: String,
    pub reason: String,
}
