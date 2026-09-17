//! Claude IPC stays separate from Codex and uses the same desktop command implementations.
mod integration_api;
mod providers_api;
use super::http::{argument, as_json, InvokeRequest};
use crate::commands::error::CommandError;
use serde_json::Value;
use tauri::AppHandle;

pub(super) async fn dispatch(
    app: &AppHandle,
    request: &InvokeRequest,
) -> Result<Option<Value>, CommandError> {
    if let Some(value) = integration_api::dispatch(app, request).await? {
        return Ok(Some(value));
    }
    providers_api::dispatch(app, request).await
}