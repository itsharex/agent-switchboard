use super::*;

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
        "list_claude_presets" => as_json(crate::commands::claude_management::list_claude_presets()),
        "prepare_claude_preset" => {
            as_json(crate::commands::claude_management::prepare_claude_preset(
                argument(&request.args, "presetId")?,
                argument(&request.args, "apiKey")?,
                argument(&request.args, "variables")?,
                argument(&request.args, "accountId")?,
            ))
        }
        "duplicate_claude_profile" => command!(
            crate::commands::claude_management::duplicate_claude_profile(
                app.clone(),
                argument(&request.args, "profileId")?,
                argument(&request.args, "name")?,
                argument(&request.args, "expectedFileHash")?,
                argument(&request.args, "confirmWrite")?
            )
        ),
        "search_claude_profiles" => {
            command!(crate::commands::claude_management::search_claude_profiles(
                app.clone(),
                argument(&request.args, "query")?
            ))
        }
        _ => return Ok(None),
    };
    result.map(Some)
}
