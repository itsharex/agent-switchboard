use super::error::{blocking, observe, state, CommandError};
use super::window::apply_desktop_settings;
use crate::local_state::AppSettings;
use crate::runtime_log::RuntimeLogAction;
use crate::desktop_shortcut::DesktopSettingsState;
use tauri::Manager;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsSnapshot {
    settings: AppSettings,
    desktop_error: Option<String>,
}

/// Reads application-runtime settings. This is deliberately separate from the
/// Codex / Claude common configuration contract.
#[tauri::command]
pub async fn get_app_settings(app: tauri::AppHandle) -> Result<AppSettingsSnapshot, CommandError> {
    let desktop = app.state::<DesktopSettingsState>();
    let _guard = desktop.gate.lock().await;
    let state = state(&app)?;
    let settings = blocking(move || {
        state
            .get_app_settings()
            .map_err(|error| CommandError::new("app-settings-unavailable", error))
    })
    .await?;
    Ok(AppSettingsSnapshot { settings, desktop_error: desktop.error()? })
}

/// Updates application-runtime settings and returns the persisted value.
#[tauri::command]
pub async fn set_app_settings(
    app: tauri::AppHandle,
    settings: AppSettings,
) -> Result<AppSettings, CommandError> {
    let desktop = app.state::<DesktopSettingsState>();
    let _guard = desktop.gate.lock().await;
    let refresh_app = app.clone();
    let write_app = app.clone();
    let saved = observe(RuntimeLogAction::AppSettingsSaved, async move {
        let app = write_app;
        let current_state = state(&app)?;
        settings
            .validate()
            .map_err(|error| CommandError::new("app-settings-invalid", error))?;
        let previous = blocking(move || {
            current_state
                .get_app_settings()
                .map_err(|error| CommandError::new("app-settings-unavailable", error))
        })
        .await?;
        if let Err(error) = apply_desktop_settings(&app, &settings) {
            return Err(restore_desktop_settings(&app, &previous, error));
        }
        let state = state(&app)?;
        let saved = blocking(move || {
            state
                .set_app_settings(&settings)
                .map_err(|error| CommandError::new("app-settings-save-failed", error))?;
            // The cache is not a second setting store: it applies the
            // just-persisted threshold to this result and later events.
            crate::runtime_log::set_level(settings.runtime_log_level);
            Ok(settings)
        })
        .await;
        saved.map_err(|error| restore_desktop_settings(&app, &previous, error))
    })
    .await?;
    desktop.set_error(None);
    crate::tray::refresh(&refresh_app);
    Ok(saved)
}

/// One-click repair for an invalid app-settings file: replaces it with
/// validated defaults. Persisting runs before the live desktop settings are
/// applied so a repair is never lost to a window-state failure.
#[tauri::command]
pub async fn repair_app_settings(app: tauri::AppHandle) -> Result<AppSettingsSnapshot, CommandError> {
    let desktop = app.state::<DesktopSettingsState>();
    let _guard = desktop.gate.lock().await;
    let app_for_apply = app.clone();
    let repair_app = app.clone();
    let repaired = observe(RuntimeLogAction::AppSettingsRepaired, async move {
        let state = state(&repair_app)?;
        blocking(move || {
            state
                .repair_app_settings()
                .map_err(|error| CommandError::new("app-settings-repair-failed", error))
        })
        .await
    })
    .await?;
    match apply_desktop_settings(&app_for_apply, &repaired) {
        Ok(()) => desktop.set_error(None),
        Err(error) => {
            log::error!("设置文件已修复，但桌面偏好应用失败：{}", error.message);
            desktop.set_error(Some(error.message));
        }
    }
    crate::runtime_log::set_level(repaired.runtime_log_level);
    crate::tray::refresh(&app_for_apply);
    Ok(AppSettingsSnapshot { settings: repaired, desktop_error: desktop.error()? })
}

fn restore_desktop_settings(app: &tauri::AppHandle, previous: &AppSettings, error: CommandError) -> CommandError {
    let error = match apply_desktop_settings(app, previous) {
        Ok(()) => error,
        Err(rollback) => CommandError::new("desktop-settings-rollback-failed", format!(
            "{}；恢复原桌面偏好失败，请重启应用：{}", error.message, rollback.message,
        )),
    };
    app.state::<DesktopSettingsState>().set_error(Some(error.message.clone()));
    error
}

/// Installed system font families, offered by the interface-font picker.
#[tauri::command]
pub async fn list_system_fonts() -> Result<Vec<String>, CommandError> {
    blocking(|| Ok(crate::fonts::system_font_families())).await
}
