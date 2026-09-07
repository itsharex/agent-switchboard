//! Codex `config.toml` adapter.
//!
//! Pure parse / preview / render over configuration text. Host-owned keys,
//! comments and layout are preserved by editing the document through
//! `toml_edit` instead of re-serializing from a typed mirror.

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
pub(crate) use state::{owned_diff, uses_builtin_provider};
pub use state::{route_state, OFFICIAL_PROVIDER};
