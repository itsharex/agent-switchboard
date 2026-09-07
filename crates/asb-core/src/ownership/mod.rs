//! Ownership directory: the single authority for which configuration keys
//! Agent Switchboard is allowed to read, patch, or write.
//!
//! Every key outside these sets is host-owned and must be preserved
//! byte-for-byte. Patches referencing a host-owned key are rejected at
//! validation time, never silently dropped.

mod choices;
mod directory;
mod lookup;
mod provider;
mod spec;
#[cfg(test)]
mod tests;
mod toggles;

pub use choices::{CLAUDE_CHOICES, CODEX_CHOICES};
pub use lookup::{
    choice_spec, common_choices, common_groups, common_toggles, default_common_settings, is_owned,
    is_provider_owned, official_setting_directory, owner_for, provider_absent_action, setting_spec,
    setting_specs, toggle_spec,
};
pub use provider::{
    CODEX_LEGACY_PROVIDER_BASE_URL_KEY, CODEX_LEGACY_PROVIDER_ID, CODEX_LEGACY_PROVIDER_NAME_KEY,
    CODEX_LEGACY_PROVIDER_TOKEN_KEY, CODEX_LEGACY_PROVIDER_WIRE_API_KEY, CODEX_WEB_SEARCH_KEY,
};
pub use spec::{
    ChoiceControl, ChoiceOption, ChoiceSpec, OfficialSettingDisposition, OfficialSettingEntry,
    ProviderAbsentAction, SettingControl, SettingOwner, SettingSpec, SettingValueType, ToggleSpec,
};
pub use toggles::{CLAUDE_TOGGLES, CODEX_TOGGLES};
