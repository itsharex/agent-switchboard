//! Codex gateway IPC forwarding; business logic stays in the typed command owner.
use super::super::http::{argument, as_json, InvokeRequest};
use crate::commands::{self, error::CommandError};
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
    match request.command.as_str() {
        "get_codex_gateway_policy" => command!(
            commands::switching::codex_policy::get_codex_gateway_policy(app.clone())
        ),
        "prepare_codex_gateway_policy" => command!(
            commands::switching::codex_policy::prepare_codex_gateway_policy(
                app.clone(),
                argument(&request.args, "profileId")?,
                argument(&request.args, "policy")?
            )
        ),
        "commit_codex_gateway_policy" => command!(
            commands::switching::codex_policy::commit_codex_gateway_policy(
                app.clone(),
                argument(&request.args, "preparationId")?,
                argument(&request.args, "confirmWrite")?
            )
        ),
        "cancel_codex_gateway_policy" => command!(
            commands::switching::codex_policy::cancel_codex_gateway_policy(
                app.clone(),
                argument(&request.args, "preparationId")?
            )
        ),
        "recover_codex_gateway_policy" => {
            command!(commands::switching::codex_policy::recover_codex_gateway_policy(app.clone()))
        }
        "discard_codex_gateway_policy" => command!(
            commands::switching::codex_policy::discard_codex_gateway_policy(
                app.clone(),
                argument(&request.args, "expectedConfigHash")?,
                argument(&request.args, "confirmWrite")?
            )
        ),
        "reset_codex_provider_health" => command!(
            commands::switching::codex_policy::reset_codex_provider_health(
                app.clone(),
                argument(&request.args, "profileId")?,
                argument(&request.args, "confirmWrite")?
            )
        ),
        _ => Ok(None),
    }
}
