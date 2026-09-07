//! Typed data contracts shared by the Rust core, the switch executor,
//! persistence, and the UI bindings.
//!
//! This module is the single owner of every field list. Nothing else may
//! redefine these shapes.

mod kinds;
mod model;
mod plan;
mod provider;
mod settings;
#[cfg(test)]
mod tests;
mod usage;

pub use kinds::{AppKind, AuthenticationScheme, GlobalPromptDocument, RouteMode, UpstreamProtocol};
pub use model::{ClaudeModelSettings, CodexModelSettings, ExplicitMaxOutputTokens, ModelOptions};
pub use plan::{
    BackupRecord, ChangeKind, ConfigWriteRecord, ImportProposal, KeyChange, MatchStatus,
    RouteState, SwitchPlan, SwitchPreview, WriteOperation,
};
pub use provider::{
    classify_profile_save, ProfileSaveKind, ProviderDraft, ProviderFile, ProviderProfile,
    ProviderRecord,
};
pub use settings::{
    CommonSettingValue, CommonSettings, CommonSettingsPreview, CommonSettingsSnapshot, ConfigValue,
};
pub use usage::{
    CodexOfficialQuota, CodexOfficialQuotaReset, CodexOfficialQuotaResetKind,
    CodexOfficialQuotaStatus, CodexOfficialQuotaWindow, ModelUsageDay, ModelUsageFreshness,
    ModelUsageGroup, ModelUsageIssue, ModelUsageRange, ModelUsageRead, ModelUsageReport,
    ModelUsageRequest, ModelUsageTokens, UsageHistoryMetric, UsageHistoryPoint,
    UsageHistoryRequest, UsageHistorySeries, UsageQuery, UsageReading, UsageSummary,
};
