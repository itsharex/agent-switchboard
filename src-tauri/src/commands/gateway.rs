//! Read-only gateway observation for the status page: listener facts, active
//! routes, and in-memory request telemetry. The gateway module owns these
//! facts; this boundary adds no second shape.

use super::error::{blocking, CommandError};
use crate::gateway::{GatewayController, GatewayObservation};
use tauri::{AppHandle, Manager};

#[tauri::command]
pub async fn gateway_status(app: AppHandle) -> Result<GatewayObservation, CommandError> {
    let gateway = app.state::<GatewayController>().inner().clone();
    blocking(move || {
        gateway
            .observe()
            .map_err(|error| CommandError::new("gateway-unavailable", error))
    })
    .await
}
