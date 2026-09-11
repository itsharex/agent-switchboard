//! asb-core owns the typed contracts and all pure planning logic.
//!
//! Everything in this crate is side-effect free: it transforms configuration
//! text into previews and rendered candidates, and it never touches the
//! filesystem. Live-file mutation belongs exclusively to `asb-switch`.

pub mod adapter;
pub mod ccswitch;
mod claude_model;
pub mod contracts;
pub mod discovery;
pub mod endpoint;
pub mod extensions;
pub mod lock;
pub mod ownership;
pub mod redact;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod validate;
pub mod website_assembly;

pub use discovery::{discover, inspect, DiscoveredFile, DiscoveredState, DiscoveryReport};

pub use ccswitch::{map_row, CcSwitchProposal, CcSwitchProviderDraft, CcSwitchRow, CcSwitchSkip};

pub use adapter::{preview, render, route_state, validate_syntax, AdapterError};
pub use contracts::{
    AppKind, AuthenticationScheme, BackupRecord, ChangeKind, ClaudeModelSettings,
    ClientSettingsPreview, ClientSettingsSnapshot, CodexCapabilities, CodexCatalogEntry,
    CodexEndpoint, CodexModelRoute, CodexModelSettings, CodexOperation, CodexProviderDraft,
    CodexProviderFile, CodexProviderProfile, CodexProviderRecord, CodexRouteSnapshot,
    CodexSubagentKey, CodexSubagentSettings, CodexSubagentSettingsPreview,
    CodexSubagentSettingsSnapshot, CodexUpstream, ConfigValue, ConfigWriteRecord,
    ExplicitMaxOutputTokens, GlobalPromptDocument, KeyChange, MatchStatus, ModelOptions,
    ProviderDraft, ProviderFile, ProviderProfile, ProviderRecord, RouteMode, RouteState,
    SettingValue, SettingsValues, SubagentSettingsPlan, SwitchPlan, SwitchPreview,
    UpstreamProtocol, WriteOperation, CODEX_PROVIDER_SCHEMA_VERSION,
};
pub use lock::{classify_lock, LockFileData, LockHolder, LockStatus, PidLiveness};
pub use redact::{redact, REDACTED};
pub use validate::{validate_plan, ValidationError};
