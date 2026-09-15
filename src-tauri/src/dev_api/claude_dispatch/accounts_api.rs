use super::*;
use crate::commands::claude_accounts as accounts;

pub(super) async fn dispatch(
    app: &AppHandle,
    request: &InvokeRequest,
) -> Result<Option<Value>, CommandError> {
    macro_rules! command {
        ($future:expr) => {
            as_json($future.await)
        };
    }
    let result = match request.command.as_str() {
        "get_claude_account_models" => command!(accounts::get_claude_account_models(
            app.clone(),
            argument(&request.args, "provider")?,
            argument(&request.args, "accountId")?
        )),
        "get_claude_account_quota" => command!(accounts::get_claude_account_quota(
            app.clone(),
            argument(&request.args, "provider")?,
            argument(&request.args, "accountId")?
        )),
        "get_claude_native_quota" => command!(accounts::get_claude_native_quota()),
        "get_claude_accounts" => command!(accounts::get_claude_accounts(app.clone())),
        "save_claude_account" => command!(accounts::save_claude_account(
            app.clone(),
            argument(&request.args, "account")?,
            argument(&request.args, "expectedFileHash")?,
            argument(&request.args, "makeDefault")?,
            argument(&request.args, "confirmWrite")?
        )),
        "set_claude_default_account" => command!(accounts::set_claude_default_account(
            app.clone(),
            argument(&request.args, "provider")?,
            argument(&request.args, "accountId")?,
            argument(&request.args, "expectedFileHash")?,
            argument(&request.args, "confirmWrite")?
        )),
        "remove_claude_account" => command!(accounts::remove_claude_account(
            app.clone(),
            argument(&request.args, "accountId")?,
            argument(&request.args, "expectedFileHash")?,
            argument(&request.args, "confirmWrite")?
        )),
        "start_claude_account_login" => command!(accounts::start_claude_account_login(
            app.clone(),
            argument(&request.args, "request")?,
            argument(&request.args, "confirmWrite")?
        )),
        "poll_claude_account_login" => command!(accounts::poll_claude_account_login(
            app.clone(),
            argument(&request.args, "sessionId")?
        )),
        "cancel_claude_account_login" => command!(accounts::cancel_claude_account_login(
            app.clone(),
            argument(&request.args, "sessionId")?
        )),
        _ => return Ok(None),
    };
    result.map(Some)
}
