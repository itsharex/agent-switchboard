//! Codex-only managed account commands. Renderer values never contain tokens.
use super::{
    error::{blocking, require_write_confirmation, state, CommandError},
    ConfigWriteGate,
};
use crate::codex_auth::{
    self,
    contracts::{AccountSelection, AccountsView},
    AccountModels, AccountQuota, LoginPoll,
};
use tauri::{AppHandle, Manager};

fn error(message: String) -> CommandError {
    CommandError::new("codex-account-operation-failed", message)
}

#[tauri::command]
pub(crate) async fn list_codex_accounts(app: AppHandle) -> Result<AccountsView, CommandError> {
    let state = state(&app)?;
    blocking(move || codex_auth::list_accounts(state.root()).map_err(error)).await
}
#[tauri::command]
pub(crate) async fn import_codex_native_account(
    app: AppHandle,
    expected_revision: String,
    confirm_write: bool,
) -> Result<AccountsView, CommandError> {
    require_write_confirmation(confirm_write, "导入 Codex 原生登录到本地账号库")?;
    let state = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        let target = state
            .target(asb_core::AppKind::Codex)
            .map_err(error)?
            .with_file_name("auth.json");
        codex_auth::import_native(state.root(), &target, &expected_revision).map_err(error)
    })
    .await
}
#[tauri::command]
pub(crate) async fn set_codex_default_account(
    app: AppHandle,
    account_id: Option<String>,
    expected_revision: String,
) -> Result<AccountsView, CommandError> {
    let state = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        codex_auth::set_default(state.root(), account_id.as_deref(), &expected_revision)
            .map_err(error)
    })
    .await
}
#[tauri::command]
pub(crate) async fn set_codex_account_binding(
    app: AppHandle,
    profile_id: String,
    selection: AccountSelection,
    expected_revision: String,
) -> Result<AccountsView, CommandError> {
    let state = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        let profile = state
            .configuration()
            .find_provider(&profile_id)
            .map_err(|_| error("Codex 官方档案不存在".into()))?;
        if profile.app != asb_core::AppKind::Codex
            || profile.route_mode != asb_core::RouteMode::Official
        {
            return Err(error("只有 Codex 官方登录档案可以绑定 ChatGPT 账号".into()));
        }
        codex_auth::set_binding(state.root(), &profile_id, selection, &expected_revision)
            .map_err(error)
    })
    .await
}
#[tauri::command]
pub(crate) async fn delete_codex_account(
    app: AppHandle,
    account_id: String,
    expected_revision: String,
    confirm_write: bool,
) -> Result<AccountsView, CommandError> {
    require_write_confirmation(confirm_write, "删除 Codex 托管账号（不退出原生登录）")?;
    let state = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        codex_auth::delete_account(state.root(), &account_id, &expected_revision).map_err(error)
    })
    .await
}
#[tauri::command]
pub(crate) async fn start_codex_account_login(
    app: AppHandle,
    account_id: Option<String>,
) -> Result<codex_auth::AccountLoginStart, CommandError> {
    let state = state(&app)?;
    blocking(move || codex_auth::start_login(state.root(), account_id).map_err(error)).await
}
#[tauri::command]
pub(crate) async fn poll_codex_account_login(
    app: AppHandle,
    session_id: String,
) -> Result<LoginPoll, CommandError> {
    let state = state(&app)?;
    blocking(move || codex_auth::poll_login(state.root(), &session_id).map_err(error)).await
}
#[tauri::command]
pub(crate) async fn cancel_codex_account_login(
    app: AppHandle,
    session_id: String,
) -> Result<(), CommandError> {
    let state = state(&app)?;
    blocking(move || codex_auth::cancel_login(state.root(), &session_id).map_err(error)).await
}
#[tauri::command]
pub(crate) async fn get_codex_account_models(
    app: AppHandle,
    account_id: Option<String>,
) -> Result<AccountModels, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let path = state
            .target(asb_core::AppKind::Codex)
            .map_err(error)?
            .with_file_name("auth.json");
        codex_auth::models(state.root(), account_id.as_deref(), &path).map_err(error)
    })
    .await
}
#[tauri::command]
pub(crate) async fn get_codex_account_quota(
    app: AppHandle,
    account_id: Option<String>,
) -> Result<AccountQuota, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let path = state
            .target(asb_core::AppKind::Codex)
            .map_err(error)?
            .with_file_name("auth.json");
        codex_auth::quota(state.root(), account_id.as_deref(), &path).map_err(error)
    })
    .await
}

#[tauri::command]
pub(crate) async fn get_codex_auth_policy(
    app: AppHandle,
) -> Result<codex_auth::policy::AuthPolicyView, CommandError> {
    let state = state(&app)?;
    blocking(move || codex_auth::policy::load(state.root()).map_err(error)).await
}
#[tauri::command]
pub(crate) async fn set_codex_auth_policy(
    app: AppHandle,
    preserve_official_login: bool,
    expected_revision: String,
) -> Result<codex_auth::policy::AuthPolicyView, CommandError> {
    let state = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        codex_auth::policy::save(state.root(), preserve_official_login, &expected_revision)
            .map_err(error)
    })
    .await
}
