//! Claude Code `settings.json` adapter.
//!
//! Pure parse / preview / render over JSON text. Key order is preserved via
//! `serde_json` with the `preserve_order` feature; secrets are never written.
//! Claude Code keeps ownership of its existing login and credential
//! environment.

mod document;
mod overlay;
mod preview;
mod render;
mod state;
#[cfg(test)]
mod tests;

pub(crate) use document::{check_syntax, parse};
pub(crate) use preview::preview;
pub(crate) use render::{render, render_common_settings};
pub use state::route_state;
pub(crate) use state::{matches_provider_credentials, owned_diff};
