use super::http::{argument, as_json, InvokeRequest};
use crate::commands::{self, error::CommandError, workspace};
use crate::local_state::AppSettings;
use serde_json::Value;
use tauri::AppHandle;

pub(super) async fn dispatch(app: &AppHandle, request: &InvokeRequest) -> Result<Option<Value>, CommandError> {
    let result = match request.command.as_str() {
        "get_app_settings" => as_json(commands::get_app_settings(app.clone()).await),
        "set_app_settings" => as_json(commands::set_app_settings(
            app.clone(), argument::<AppSettings>(&request.args, "settings")?,
        ).await),
        "repair_app_settings" => as_json(commands::repair_app_settings(app.clone()).await),
        "list_system_fonts" => as_json(commands::list_system_fonts().await),
        "get_startup_page" => as_json(workspace::get_startup_page(app.clone()).await),
        "remember_workspace_page" => as_json(workspace::remember_workspace_page(
            app.clone(), argument(&request.args, "page")?,
        ).await),
        "set_shortcut_recording" => {
            workspace::set_shortcut_recording(app.clone(), argument(&request.args, "recording")?);
            as_json(Ok(()))
        }
        _ => return Ok(None),
    };
    result.map(Some)
}
