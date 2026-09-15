//! Codex debug IPC uses the same commands as the desktop app.
mod accounts;
mod gateway;
mod metering;
mod prompts;
mod providers;
mod settings;
use super::http::InvokeRequest;
use crate::commands::error::CommandError;
use serde_json::Value;
use tauri::AppHandle;

pub(super) async fn dispatch(
    app: &AppHandle,
    request: &InvokeRequest,
) -> Result<Option<Value>, CommandError> {
    if !request.command.contains("codex") {
        return Ok(None);
    }
    if let Some(value) = prompts::dispatch(app, request).await? {
        return Ok(Some(value));
    }
    if let Some(value) = accounts::dispatch(app, request).await? {
        return Ok(Some(value));
    }
    if let Some(value) = providers::dispatch(app, request).await? {
        return Ok(Some(value));
    }
    if let Some(value) = gateway::dispatch(app, request).await? {
        return Ok(Some(value));
    }
    if let Some(value) = metering::dispatch(app, request).await? {
        return Ok(Some(value));
    }
    settings::dispatch(app, request).await
}
