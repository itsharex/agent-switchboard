use super::error::{blocking, state, CommandError};
use crate::desktop_shortcut::DesktopSettingsState;
use crate::local_state::{StartupPage, WorkspacePage};
use tauri::Manager;

#[tauri::command]
pub async fn get_startup_page(app: tauri::AppHandle) -> Result<WorkspacePage, CommandError> {
    let local = state(&app)?;
    blocking(move || {
        let settings = local.get_app_settings()
            .map_err(|error| CommandError::new("app-settings-unavailable", error))?;
        match settings.startup_page {
            StartupPage::Providers => Ok(WorkspacePage::Providers),
            StartupPage::LastVisited => local.last_workspace_page()
                .map_err(|error| CommandError::new("workspace-page-unavailable", error)),
        }
    }).await
}

#[tauri::command]
pub async fn remember_workspace_page(app: tauri::AppHandle, page: WorkspacePage) -> Result<(), CommandError> {
    let desktop = app.state::<DesktopSettingsState>();
    let _guard = desktop.gate.lock().await;
    let local = state(&app)?;
    blocking(move || local.remember_workspace_page(page)
        .map_err(|error| CommandError::new("workspace-page-save-failed", error))).await
}

#[tauri::command]
pub fn set_shortcut_recording(app: tauri::AppHandle, recording: bool) {
    app.state::<DesktopSettingsState>().set_recording(recording);
}
