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
pub(crate) use bindings::{binding, clear_bindings, require_unbound, set_binding};
pub(crate) use login::{cancel_login, poll_login, start_login};
pub(crate) use manager::{
    delete_account, import_native, list_accounts, set_default, valid_account,
};
pub(crate) use query::{models, quota};
pub(crate) use store::lock;
pub(crate) mod projection;
#[cfg(test)]
mod tests;
pub(crate) use login::{AccountLoginStart, LoginPoll};
pub(crate) use query::{AccountModels, AccountQuota};
pub(crate) mod policy;
mod quota_history;
mod native_sync;
