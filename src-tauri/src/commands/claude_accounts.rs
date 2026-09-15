//! Claude upstream accounts and device logins. Native Claude/Codex logins stay separate.

use super::error::{blocking, require_write_confirmation, state, CommandError};
use crate::claude_auth::device::{ClaudeLoginRequest, ClaudeLoginView};
use crate::claude_auth::{ClaudeAccount, ClaudeAccountsView, ClaudeAuth};
use asb_core::claude_auth::ClaudeAuthProvider;
use tauri::AppHandle;

fn failure(message: String) -> CommandError {
    CommandError::new("claude-account-unavailable", message)
}

#[tauri::command]
pub(crate) async fn get_claude_accounts(
    app: AppHandle,
) -> Result<ClaudeAccountsView, CommandError> {
    let local = state(&app)?;
    blocking(move || ClaudeAuth::shared(local.root()).view().map_err(failure)).await
}

#[tauri::command]
pub(crate) async fn save_claude_account(
    app: AppHandle,
    account: ClaudeAccount,
    expected_file_hash: String,
    make_default: bool,
    confirm_write: bool,
) -> Result<ClaudeAccountsView, CommandError> {
    require_write_confirmation(confirm_write, "保存 Claude 托管账号")?;
    let local = state(&app)?;
    blocking(move || {
        ClaudeAuth::shared(local.root())
            .save_account(account, &expected_file_hash, make_default)
            .map_err(failure)
    })
    .await
}

#[tauri::command]
pub(crate) async fn set_claude_default_account(
    app: AppHandle,
    provider: ClaudeAuthProvider,
    account_id: String,
    expected_file_hash: String,
    confirm_write: bool,
) -> Result<ClaudeAccountsView, CommandError> {
    require_write_confirmation(confirm_write, "切换 Claude 默认托管账号")?;
    let local = state(&app)?;
    blocking(move || {
        ClaudeAuth::shared(local.root())
            .set_default(provider, &account_id, &expected_file_hash)
            .map_err(failure)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remove_claude_account(
    app: AppHandle,
    account_id: String,
    expected_file_hash: String,
    confirm_write: bool,
) -> Result<ClaudeAccountsView, CommandError> {
    require_write_confirmation(confirm_write, "移除 Claude 托管账号")?;
    let local = state(&app)?;
    blocking(move || {
        let profiles = local
            .configuration()
            .list_providers()
            .map_err(|error| failure(error.to_string()))?;
        if profiles.iter().any(|record| {
            record.profile.app == asb_core::AppKind::Claude
                && record
                    .profile
                    .connection
                    .auth_binding
                    .as_ref()
                    .is_some_and(|binding| {
                        binding.account_id.as_deref() == Some(account_id.as_str())
                    })
        }) {
            return Err(failure(
                "此 Claude 账号仍被供应商绑定，请先更改档案中的绑定再移除".into(),
            ));
        }
        ClaudeAuth::shared(local.root())
            .remove(&account_id, &expected_file_hash)
            .map_err(failure)
    })
    .await
}

#[tauri::command]
pub(crate) async fn start_claude_account_login(
    app: AppHandle,
    request: ClaudeLoginRequest,
    confirm_write: bool,
) -> Result<ClaudeLoginView, CommandError> {
    require_write_confirmation(confirm_write, "登录并保存 Claude 托管账号")?;
    let local = state(&app)?;
    blocking(move || {
        ClaudeAuth::shared(local.root())
            .start_login(request)
            .map_err(failure)
    })
    .await
}

#[tauri::command]
pub(crate) async fn poll_claude_account_login(
    app: AppHandle,
    session_id: String,
) -> Result<ClaudeLoginView, CommandError> {
    let local = state(&app)?;
    blocking(move || {
        ClaudeAuth::shared(local.root())
            .poll_login(&session_id)
            .map_err(failure)
    })
    .await
}

#[tauri::command]
pub(crate) async fn cancel_claude_account_login(
    app: AppHandle,
    session_id: String,
) -> Result<(), CommandError> {
    let local = state(&app)?;
    blocking(move || {
        ClaudeAuth::shared(local.root())
            .cancel_login(&session_id)
            .map_err(failure)
    })
    .await
}

#[tauri::command]
pub(crate) async fn get_claude_account_models(
    app: AppHandle,
    provider: ClaudeAuthProvider,
    account_id: Option<String>,
) -> Result<Vec<crate::probe::ProviderModel>, CommandError> {
    let local = state(&app)?;
    blocking(move || {
        let options = crate::claude_auth::quota::selection(provider, account_id.as_deref());
        let account = ClaudeAuth::shared(local.root())
            .resolve(&options)
            .map_err(failure)?
            .ok_or_else(|| failure("Claude 托管认证不可用".into()))?;
        crate::claude_auth::models::fetch(&account, provider.default_protocol()).map_err(failure)
    })
    .await
}

#[tauri::command]
pub(crate) async fn get_claude_account_quota(
    app: AppHandle,
    provider: ClaudeAuthProvider,
    account_id: Option<String>,
) -> Result<crate::claude_auth::quota::ClaudeAccountQuota, CommandError> {
    let local = state(&app)?;
    blocking(move || {
        ClaudeAuth::shared(local.root())
            .quota(provider, account_id.as_deref())
            .map_err(failure)
    })
    .await
}

/// The Claude CLI's own subscription usage, read from its native login cache.
/// This is neither a managed account nor gateway metering: the credential is
/// read once, never refreshed, and never returned.
#[tauri::command]
pub(crate) async fn get_claude_native_quota(
) -> Result<crate::claude_native_quota::ClaudeNativeQuota, CommandError> {
    blocking(move || {
        let path = crate::local_state::LocalState::claude_credentials_path()
            .map_err(|error| CommandError::new("config-path-unavailable", error))?;
        crate::claude_native_quota::query(&path, crate::claude_native_quota::USAGE_URL)
            .map_err(|message| CommandError::new("claude-native-quota-unavailable", message))
    })
    .await
}
