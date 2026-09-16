//! xAI (SuperGrok) account commands for managed Codex cards. Tokens never
//! cross the IPC boundary: views carry identity and expiry only, and the
//! gateway resolves credentials per request through `xai_auth`.

use super::error::{blocking, state, CommandError};
use crate::local_state::LocalState;
use crate::xai_auth;
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XaiAccountView {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) is_default: bool,
    pub(crate) expires_at_ms: i64,
    pub(crate) bound_profile_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XaiAccountsView {
    pub(crate) revision: String,
    pub(crate) default_account_id: Option<String>,
    pub(crate) accounts: Vec<XaiAccountView>,
    pub(crate) bindings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XaiLoginPoll {
    pub(crate) status: &'static str,
}

fn store_error(error: String) -> CommandError {
    CommandError::new("xai-account-store", error)
}

fn build_view(
    accounts: Vec<xai_auth::XaiAccount>,
    default_id: Option<String>,
    bindings: BTreeMap<String, String>,
    revision: String,
) -> XaiAccountsView {
    let accounts = accounts
        .into_iter()
        .map(|account| XaiAccountView {
            bound_profile_ids: bindings
                .iter()
                .filter(|(_, bound)| bound.as_str() == account.id)
                .map(|(profile, _)| profile.clone())
                .collect(),
            is_default: default_id.as_deref() == Some(account.id.as_str()),
            id: account.id,
            label: account.label,
            expires_at_ms: account.tokens.expires_at_ms,
        })
        .collect();
    XaiAccountsView {
        revision,
        default_account_id: default_id,
        accounts,
        bindings,
    }
}

fn list(state: &LocalState) -> Result<XaiAccountsView, CommandError> {
    let (accounts, default_id, bindings, revision) =
        xai_auth::view(state.root()).map_err(store_error)?;
    Ok(build_view(accounts, default_id, bindings, revision))
}

#[tauri::command]
pub(crate) async fn list_xai_accounts(app: AppHandle) -> Result<XaiAccountsView, CommandError> {
    let state = state(&app)?;
    blocking(move || list(&state)).await
}

#[tauri::command]
pub(crate) async fn start_xai_login(
    app: AppHandle,
) -> Result<xai_auth::XaiLoginSession, CommandError> {
    let state = state(&app)?;
    blocking(move || xai_auth::start_login(state.root()).map_err(store_error)).await
}

#[tauri::command]
pub(crate) async fn poll_xai_login(app: AppHandle) -> Result<XaiLoginPoll, CommandError> {
    let state = state(&app)?;
    blocking(
        move || match xai_auth::poll_login(state.root()).map_err(store_error)? {
            xai_auth::XaiPollOutcome::Pending => Ok(XaiLoginPoll { status: "pending" }),
            xai_auth::XaiPollOutcome::Completed => Ok(XaiLoginPoll {
                status: "completed",
            }),
        },
    )
    .await
}

#[tauri::command]
pub(crate) async fn cancel_xai_login(app: AppHandle) -> Result<bool, CommandError> {
    let state = state(&app)?;
    blocking(move || Ok(xai_auth::cancel_login(state.root()))).await
}

#[tauri::command]
pub(crate) async fn delete_xai_account(
    app: AppHandle,
    account_id: String,
    expected_revision: String,
) -> Result<XaiAccountsView, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        xai_auth::delete_account(state.root(), &account_id, &expected_revision)
            .map_err(store_error)?;
        list(&state)
    })
    .await
}

#[tauri::command]
pub(crate) async fn set_xai_account_binding(
    app: AppHandle,
    profile_id: String,
    account_id: Option<String>,
    expected_revision: String,
) -> Result<XaiAccountsView, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        xai_auth::set_profile_binding(
            state.root(),
            &profile_id,
            account_id.as_deref(),
            &expected_revision,
        )
        .map_err(store_error)?;
        list(&state)
    })
    .await
}
