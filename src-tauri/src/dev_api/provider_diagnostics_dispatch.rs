use super::http::{argument, as_json, InvokeRequest};
use crate::commands::{error::CommandError, provider_diagnostics as commands};
use serde_json::Value;
use tauri::AppHandle;

pub(super) async fn dispatch(app: &AppHandle, request: &InvokeRequest) -> Result<Option<Value>, CommandError> {
    let value = match request.command.as_str() {
        "diagnose_provider" => as_json(commands::diagnose_provider(
            app.clone(), argument(&request.args, "profileId")?,
        ).await),
        "prepare_provider_repair" => as_json(commands::prepare_provider_repair(
            app.clone(), argument(&request.args, "profileId")?,
        ).await),
        "commit_provider_repair" => as_json(commands::commit_provider_repair(
            app.clone(), argument(&request.args, "preparationId")?, argument(&request.args, "confirmWrite")?,
        ).await),
        "prepare_provider_repair_undo" => as_json(commands::prepare_provider_repair_undo(
            app.clone(), argument(&request.args, "repairId")?,
        ).await),
        "commit_provider_repair_undo" => as_json(commands::commit_provider_repair_undo(
            app.clone(), argument(&request.args, "preparationId")?, argument(&request.args, "confirmWrite")?,
        ).await),
        "cancel_provider_repair_preparation" => as_json(commands::cancel_provider_repair_preparation(
            app.clone(), argument(&request.args, "preparationId")?,
        ).await),
        _ => return Ok(None),
    }?;
    Ok(Some(value))
}
