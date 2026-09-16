//! Claude Code `settings.json` adapter.
//!
//! Pure parse / preview / render over JSON text. Key order is preserved via
//! `serde_json` with the `preserve_order` feature; secrets are never written.
//! Claude Code keeps ownership of its existing login and credential
//! environment.

mod common;
mod document;
mod gateway;
mod native;
mod takeover;
pub use takeover::restore_gateway_overlay;
mod overlay;
#[cfg(test)]
mod parity_tests;
mod preview;
mod render;
mod state;
#[cfg(test)]
mod tests;

pub(crate) use document::{check_syntax, parse};
pub use common::extract_client_settings;
pub(crate) use preview::preview;
pub(crate) use render::render_gateway_base_url;
pub(crate) use render::{render, render_client_settings, render_client_settings_into_file};
pub use state::route_state;
pub(crate) use state::{matches_provider_credentials, owned_diff};
