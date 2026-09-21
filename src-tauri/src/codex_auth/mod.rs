//! Codex identities and account bindings, independent from Claude.
//! Only the switch executor may project managed credentials to live auth.
mod bindings;
pub(crate) mod contracts;
mod identity;
mod manager;
mod oauth;
mod query;
mod store;

/// The only ChatGPT Codex backend official routes may target. This is the
/// single owner of the official endpoint inside the Codex domain.
pub(crate) const OFFICIAL_CODEX_BASE: &str = "https://chatgpt.com/backend-api/codex";

pub(crate) use bindings::{binding, clear_bindings, require_unbound};
pub(crate) use manager::valid_account;
pub(crate) use query::{cached_profile_quota, official_models_document, quota};
pub(crate) use store::lock;
pub(crate) mod projection;
mod native_sync;
pub(crate) mod policy;
mod quota_history;
