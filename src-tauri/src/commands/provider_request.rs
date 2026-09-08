use super::error::{blocking, state, CommandError};
use crate::provider_request::{
    self, ProviderRequestInput, ProviderRequestPreparation, ProviderRequestResult,
    ProviderRequestTarget, ProviderRequests,
};
use tauri::Manager;

/// The only command that accepts a connection. Saved profiles send just their
/// id; an unsaved draft sends its connection once, and every later command
/// refers to it by the opaque token returned here.
#[tauri::command]
pub async fn prepare_provider_request(
    app: tauri::AppHandle,
    target: ProviderRequestTarget,
) -> Result<ProviderRequestPreparation, CommandError> {
    let local = state(&app)?;
    let requests = app.state::<ProviderRequests>().inner().clone();
    blocking(move || provider_request::prepare(&requests, &local.configuration(), target)).await
}

/// Models offered by the connection a preparation token stands for. The
/// backend resolves the credential from its prepared target, so no credential
/// travels with the command.
#[tauri::command]
pub async fn fetch_provider_request_models(
    app: tauri::AppHandle,
    request_id: String,
) -> Result<Vec<crate::probe::ProviderModel>, CommandError> {
    let local = state(&app)?;
    let requests = app.state::<ProviderRequests>().inner().clone();
    provider_request::fetch_models(requests, local.configuration(), request_id).await
}

#[tauri::command]
pub async fn execute_provider_request(
    app: tauri::AppHandle,
    request: ProviderRequestInput,
) -> Result<ProviderRequestResult, CommandError> {
    let local = state(&app)?;
    let requests = app.state::<ProviderRequests>().inner().clone();
    provider_request::execute(requests, local.configuration(), request).await
}

#[tauri::command]
pub async fn cancel_provider_request(
    app: tauri::AppHandle,
    request_id: String,
) -> Result<bool, CommandError> {
    let requests = app.state::<ProviderRequests>().inner().clone();
    requests.cancel(&request_id).await
}
