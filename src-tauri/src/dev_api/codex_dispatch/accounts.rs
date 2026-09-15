use super::super::http::{argument, as_json, optional_argument, InvokeRequest};
use crate::commands::{codex_accounts as commands, error::CommandError};
use serde_json::Value;
use tauri::AppHandle;
macro_rules! command {
    ($future:expr) => {
        as_json($future.await).map(Some)
    };
}
pub(super) async fn dispatch(
    app: &AppHandle,
    request: &InvokeRequest,
) -> Result<Option<Value>, CommandError> {
    let args = &request.args;
    match request.command.as_str() {
        "get_codex_auth_policy" => command!(commands::get_codex_auth_policy(app.clone())),
        "set_codex_auth_policy" => command!(commands::set_codex_auth_policy(
            app.clone(),
            argument(args, "preserveOfficialLogin")?,
            argument(args, "expectedRevision")?
        )),
        "list_codex_accounts" => command!(commands::list_codex_accounts(app.clone())),
        "import_codex_native_account" => command!(commands::import_codex_native_account(
            app.clone(),
            argument(args, "expectedRevision")?,
            argument(args, "confirmWrite")?
        )),
        "set_codex_default_account" => command!(commands::set_codex_default_account(
            app.clone(),
            optional_argument(args, "accountId")?,
            argument(args, "expectedRevision")?
        )),
        "set_codex_account_binding" => command!(commands::set_codex_account_binding(
            app.clone(),
            argument(args, "profileId")?,
            argument(args, "selection")?,
            argument(args, "expectedRevision")?
        )),
        "delete_codex_account" => command!(commands::delete_codex_account(
            app.clone(),
            argument(args, "accountId")?,
            argument(args, "expectedRevision")?,
            argument(args, "confirmWrite")?
        )),
        "start_codex_account_login" => command!(commands::start_codex_account_login(
            app.clone(),
            optional_argument(args, "accountId")?
        )),
        "poll_codex_account_login" => command!(commands::poll_codex_account_login(
            app.clone(),
            argument(args, "sessionId")?
        )),
        "cancel_codex_account_login" => command!(commands::cancel_codex_account_login(
            app.clone(),
            argument(args, "sessionId")?
        )),
        "get_codex_account_models" => command!(commands::get_codex_account_models(
            app.clone(),
            optional_argument(args, "accountId")?
        )),
        "get_codex_account_quota" => command!(commands::get_codex_account_quota(
            app.clone(),
            optional_argument(args, "accountId")?
        )),
        _ => Ok(None),
    }
}
