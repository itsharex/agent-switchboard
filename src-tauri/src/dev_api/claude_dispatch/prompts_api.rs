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
        "list_claude_prompts" => command!(crate::commands::claude_prompts::list_claude_prompts(
            app.clone()
        )),
        "save_claude_prompt" => command!(crate::commands::claude_prompts::save_claude_prompt(
            app.clone(),
            argument(&request.args, "promptId")?,
            argument(&request.args, "draft")?,
            argument(&request.args, "expectedFileHash")?,
            argument(&request.args, "confirmWrite")?
        )),
        "remove_claude_prompt" => command!(crate::commands::claude_prompts::remove_claude_prompt(
            app.clone(),
            argument(&request.args, "promptId")?,
            argument(&request.args, "expectedFileHash")?,
            argument(&request.args, "confirmWrite")?
        )),
        "reorder_claude_prompts" => {
            command!(crate::commands::claude_prompts::reorder_claude_prompts(
                app.clone(),
                argument(&request.args, "orderedIds")?,
                argument(&request.args, "expectedFileHash")?,
                argument(&request.args, "confirmWrite")?
            ))
        }
        "preview_claude_prompt" => {
            command!(crate::commands::claude_prompts::preview_claude_prompt(
                app.clone(),
                argument(&request.args, "promptId")?,
                argument(&request.args, "expectedFileHash")?
            ))
        }
        "activate_claude_prompt" => {
            command!(crate::commands::claude_prompts::activate_claude_prompt(
                app.clone(),
                argument(&request.args, "plan")?,
                argument(&request.args, "confirmWrite")?
            ))
        }
        "recover_claude_prompt" => {
            command!(crate::commands::claude_prompts::recover_claude_prompt(
                app.clone(),
                argument(&request.args, "confirmWrite")?
            ))
        }
        "scan_claude_prompt_source" => {
            command!(crate::commands::claude_prompts::scan_claude_prompt_source(
                argument(&request.args, "sourcePath")?
            ))
        }
        "import_claude_prompt_source" => command!(
            crate::commands::claude_prompts::import_claude_prompt_source(
                app.clone(),
                argument(&request.args, "sourcePath")?,
                argument(&request.args, "sourceIds")?,
                argument(&request.args, "sourceRevision")?,
                argument(&request.args, "expectedFileHash")?,
                argument(&request.args, "confirmWrite")?
            )
        ),
        _ => return Ok(None),
    };
    result.map(Some)
}
