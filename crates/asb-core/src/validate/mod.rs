//! Early, loud validation for provider profiles and general settings.
//!
//! Invalid shapes fail here with an explanation of how to fix them; the
//! switch executor refuses to receive anything that has not passed.

mod error;
mod profile;
mod settings;
#[cfg(test)]
mod tests;
mod usage;

pub use error::{ValidationError, MAX_AUTO_REFRESH_INTERVAL_MINUTES};
pub use usage::validate_usage_query;

use crate::contracts::{CommonSettings, ProviderProfile};
/// Validates the only current planning contract. Client selection comes from
/// the provider profile, so the common settings cannot carry a second,
/// conflicting `app` value.
pub fn validate_plan(
    profile: &ProviderProfile,
    common: &CommonSettings,
) -> Result<(), ValidationError> {
    profile.validate()?;
    common.validate_for(profile.app)
}
