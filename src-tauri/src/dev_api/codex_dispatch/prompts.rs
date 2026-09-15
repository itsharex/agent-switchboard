use super::super::http::{argument, as_json, optional_argument, InvokeRequest};
use crate::commands::{codex_prompts as commands, error::CommandError};
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
        "list_codex_prompts" => command!(commands::list_codex_prompts(app.clone())),
        "save_codex_prompt" => command!(commands::save_codex_prompt(
            app.clone(),
            optional_argument(args, "presetId")?,
            argument(args, "draft")?,
            argument(args, "expectedRevision")?,
            optional_argument(args, "expectedLiveHash")?,
            argument(args, "confirmWrite")?
        )),
        "delete_codex_prompt" => command!(commands::delete_codex_prompt(
            app.clone(),
            argument(args, "presetId")?,
            argument(args, "expectedRevision")?,
            argument(args, "confirmWrite")?
        )),
        "preview_codex_prompt" => command!(commands::preview_codex_prompt(
            app.clone(),
            optional_argument(args, "presetId")?,
            argument(args, "expectedRevision")?
        )),
        "apply_codex_prompt" => command!(commands::apply_codex_prompt(
            app.clone(),
            argument(args, "plan")?,
            argument(args, "confirmWrite")?
        )),
        "recover_codex_prompt" => command!(commands::recover_codex_prompt(
            app.clone(),
            argument(args, "confirmWrite")?
        )),
        _ => Ok(None),
    }
}
