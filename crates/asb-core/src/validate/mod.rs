//! Early, loud validation for provider profiles and general settings.
//!
//! Invalid shapes fail here with an explanation of how to fix them; the
//! switch executor refuses to receive anything that has not passed.

mod error;
mod claude;
mod codex;
mod profile;
#[cfg(test)]
mod responses_tests;
mod settings;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod tests;
mod usage;

pub use error::{ValidationError, MAX_AUTO_REFRESH_INTERVAL_MINUTES};
pub use usage::validate_usage_query;
pub(crate) use codex::validate_connection as validate_codex_connection;

use crate::contracts::{ProviderProfile, SettingsValues};
/// Validates the only current planning contract. Client selection comes from
/// the provider profile, so the client settings cannot carry a second,
/// conflicting `app` value.
pub fn validate_plan(
    profile: &ProviderProfile,
    client_settings: &SettingsValues,
) -> Result<(), ValidationError> {
    profile.validate()?;
    client_settings.validate_client_settings(profile.app)
}
