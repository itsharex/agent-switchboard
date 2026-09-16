use super::{
    error::{blocking, state, CommandError},
    ConfigWriteGate,
};
use crate::codex_common::{self, CommonView};
use asb_core::{AppKind, SettingsValues};
use tauri::{AppHandle, Manager};
fn error(message: String) -> CommandError {
    CommandError::new("codex-common-config-invalid", message)
}
#[tauri::command]
pub(crate) async fn get_codex_common_config(app: AppHandle) -> Result<CommonView, CommandError> {
    let state = state(&app)?;
    blocking(move || codex_common::view(&state).map_err(error)).await
}
#[tauri::command]
pub(crate) async fn extract_codex_common_config(
    app: AppHandle,
) -> Result<SettingsValues, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        codex_common::extract(&state.target(AppKind::Codex).map_err(error)?).map_err(error)
    })
    .await
}
#[tauri::command]
pub(crate) async fn set_codex_common_config_enabled(
    app: AppHandle,
    profile_id: String,
    enabled: bool,
    expected_revision: String,
) -> Result<CommonView, CommandError> {
    let state = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        codex_common::set_enabled(&state, &profile_id, enabled, &expected_revision).map_err(error)
    })
    .await
}
#[tauri::command]
pub(crate) async fn save_codex_common_fragment(
    app: AppHandle,
    text: String,
    expected_revision: String,
) -> Result<CommonView, CommandError> {
    let state = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(error)?;
        codex_common::save_fragment(&state, &text, &expected_revision).map_err(error)?;
        codex_common::view(&state).map_err(error)
    })
    .await
}
