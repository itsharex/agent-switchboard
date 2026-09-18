//! Codex identities and account bindings, independent from Claude.
//! Only the switch executor may project managed credentials to live auth.
mod bindings;
pub(crate) mod contracts;
mod identity;
mod login;
mod manager;
mod oauth;
mod query;
mod store;

/// The only ChatGPT Codex backend official routes may target. This is the
/// single owner of the official endpoint inside the Codex domain.
pub(crate) const OFFICIAL_CODEX_BASE: &str = "https://chatgpt.com/backend-api/codex";

pub(crate) use bindings::{binding, clear_bindings, require_unbound, set_binding};
pub(crate) use login::{cancel_login, poll_login, start_login};
pub(crate) use manager::{
    delete_account, import_native, list_accounts, set_default, valid_account,
};
pub(crate) use query::{models, official_models_document, quota};
pub(crate) use store::lock;
pub(crate) mod projection;
pub(crate) use login::{AccountLoginStart, LoginPoll};
pub(crate) use query::{AccountModels, AccountQuota};
mod native_sync;
pub(crate) mod policy;
mod quota_history;
