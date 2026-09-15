use super::*;
use crate::commands::claude_env as env;
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
        "scan_claude_env_conflicts" => command!(env::scan_claude_env_conflicts()),
        "remove_claude_env_conflicts" => command!(env::remove_claude_env_conflicts(
            app.clone(),
            argument(&request.args, "selections")?,
            argument(&request.args, "expectedRevision")?,
            argument(&request.args, "confirmWrite")?
        )),
        "list_claude_env_backups" => command!(env::list_claude_env_backups(app.clone())),
        "restore_claude_env_backup" => command!(env::restore_claude_env_backup(
            app.clone(),
            argument(&request.args, "fileName")?,
            argument(&request.args, "confirmWrite")?
        )),
        "get_claude_session_usage" => {
            command!(crate::commands::claude_session_usage::get_claude_session_usage(app.clone()))
        }
        "scan_claude_mcp_source" => {
            command!(crate::commands::claude_mcp_import::scan_claude_mcp_source(
                app.clone(),
                argument(&request.args, "sourcePath")?
            ))
        }
        "import_claude_mcp_source" => command!(
            crate::commands::claude_mcp_import::import_claude_mcp_source(
                app.clone(),
                argument(&request.args, "sourcePath")?,
                argument(&request.args, "sourceIds")?,
                argument(&request.args, "sourceRevision")?,
                argument(&request.args, "confirmWrite")?
            )
        ),
        "rebuild_claude_session_usage" => command!(
            crate::commands::claude_session_usage::rebuild_claude_session_usage(
                app.clone(),
                argument(&request.args, "confirmWrite")?
            )
        ),
        _ => return Ok(None),
    };
    result.map(Some)
}
