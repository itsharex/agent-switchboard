//! Typed data contracts shared by the Rust core, the switch executor,
//! persistence, and the UI bindings.
//!
//! This module is the single owner of every field list. Nothing else may
//! redefine these shapes.

mod codex;
mod kinds;
mod model;
mod plan;
mod provider;
mod responses;
mod settings;
mod subagents;
#[cfg(test)]
mod tests;
mod usage;

pub use codex::{
    codex_model_catalog_document, CodexCapabilities, CodexCatalogEntry, CodexChatEffortMode,
    CodexChatEffortParameter, CodexChatReasoning, CodexChatThinkingParameter, CodexEndpoint,
    CodexModelRoute, CodexOperation, CodexProviderDraft, CodexProviderFile, CodexProviderProfile,
    CodexProviderRecord, CodexReasoningLevel, CodexRouteSnapshot, CodexUpstream,
    CODEX_PROVIDER_SCHEMA_VERSION,
};
pub use kinds::{AppKind, AuthenticationScheme, GlobalPromptDocument, RouteMode, UpstreamProtocol};
pub use model::{ClaudeModelSettings, CodexModelSettings, ExplicitMaxOutputTokens, ModelOptions};
pub use plan::{
    BackupRecord, ChangeKind, ConfigWriteRecord, KeyChange, MatchStatus, RouteState, SwitchPlan,
    SwitchPreview, WriteOperation,
};
pub use provider::{
    classify_profile_save, ProfileSaveKind, ProviderDraft, ProviderFile, ProviderProfile,
    ProviderRecord,
};
pub use responses::{ResponsesOptions, ResponsesRequestMode};
pub use settings::{
    ClientSettingsPreview, ClientSettingsSnapshot, ConfigValue, SettingValue, SettingsValues,
    MAX_EXACT_CONFIG_INTEGER,
};
pub use subagents::{
    CodexSubagentKey, CodexSubagentSettings, CodexSubagentSettingsPreview,
    CodexSubagentSettingsSnapshot, SubagentSettingsPlan,
};
pub use usage::{
    CodexOfficialQuota, CodexOfficialQuotaReset, CodexOfficialQuotaResetKind,
    CodexOfficialQuotaStatus, CodexOfficialQuotaWindow, ModelUsageDay, ModelUsageFreshness,
    ModelUsageGroup, ModelUsageIssue, ModelUsageRange, ModelUsageRead, ModelUsageReport,
    ModelUsageRequest, ModelUsageTokens, UsageHistoryMetric, UsageHistoryPoint,
    UsageHistoryRequest, UsageHistorySeries, UsageQuery, UsageReading, UsageSummary,
};
