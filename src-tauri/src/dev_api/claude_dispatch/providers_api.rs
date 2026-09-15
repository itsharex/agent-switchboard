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
        "scan_claude_snippet_source" => command!(
            crate::commands::claude_management::scan_claude_snippet_source(
                app.clone(),
                argument(&request.args, "sourcePath")?
            )
        ),
        "import_claude_snippet_source" => command!(
            crate::commands::claude_management::import_claude_snippet_source(
                app.clone(),
                argument(&request.args, "sourcePath")?,
                argument(&request.args, "sourceRevision")?,
                argument(&request.args, "expectedSettingsHash")?,
                argument(&request.args, "confirmWrite")?
            )
        ),
        "test_claude_endpoints" => {
            command!(crate::commands::claude_endpoints::test_claude_endpoints(
                argument(&request.args, "urls")?
            ))
        }
        "list_provider_endpoints" => command!(
            crate::commands::provider_endpoints::list_provider_endpoints(
                app.clone(),
                argument(&request.args, "providerId")?
            )
        ),
        "add_provider_endpoint" => {
            command!(crate::commands::provider_endpoints::add_provider_endpoint(
                app.clone(),
                argument(&request.args, "providerId")?,
                argument(&request.args, "url")?,
                argument(&request.args, "expectedFileHash")?,
                argument(&request.args, "confirmWrite")?
            ))
        }
        "remove_provider_endpoint" => command!(
            crate::commands::provider_endpoints::remove_provider_endpoint(
                app.clone(),
                argument(&request.args, "providerId")?,
                argument(&request.args, "url")?,
                argument(&request.args, "expectedFileHash")?,
                argument(&request.args, "confirmWrite")?
            )
        ),
        _ => return Ok(None),
    };
    result.map(Some)
}
