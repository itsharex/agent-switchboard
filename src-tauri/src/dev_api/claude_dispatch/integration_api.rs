use super::*;
use crate::commands::claude_integration as integration;

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
        "get_claude_integration" => command!(integration::get_claude_integration(app.clone())),
        "set_claude_integration_policy" => command!(integration::set_claude_integration_policy(
            app.clone(),
            argument(&request.args, "policy")?,
            argument(&request.args, "confirmWrite")?
        )),
        "preview_claude_integration" => command!(integration::preview_claude_integration(
            argument(&request.args, "flag")?,
            argument(&request.args, "enable")?
        )),
        "apply_claude_integration" => command!(integration::apply_claude_integration(
            app.clone(),
            argument(&request.args, "preview")?,
            argument(&request.args, "confirmWrite")?
        )),
        _ => return Ok(None),
    };
    result.map(Some)
}