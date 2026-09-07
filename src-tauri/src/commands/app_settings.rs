use super::error::{blocking, observe, state, CommandError};
use super::window::apply_desktop_settings;
use crate::local_state::AppSettings;
use crate::runtime_log::RuntimeLogAction;

/// Reads application-runtime settings. This is deliberately separate from the
/// Codex / Claude common configuration contract.
#[tauri::command]
pub async fn get_app_settings(app: tauri::AppHandle) -> Result<AppSettings, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        state
            .get_app_settings()
            .map_err(|error| CommandError::new("app-settings-unavailable", error))
    })
    .await
}

/// Updates application-runtime settings and returns the persisted value.
#[tauri::command]
pub async fn set_app_settings(
    app: tauri::AppHandle,
    settings: AppSettings,
) -> Result<AppSettings, CommandError> {
    let refresh_app = app.clone();
    let saved = observe(RuntimeLogAction::AppSettingsSaved, async move {
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
            let _ = apply_desktop_settings(&app, &previous);
            return Err(error);
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
        if saved.is_err() {
            let _ = apply_desktop_settings(&app, &previous);
        }
        saved
    })
    .await?;
    crate::tray::refresh(&refresh_app);
    Ok(saved)
}

/// One-click repair for an invalid app-settings file: replaces it with
/// validated defaults. Persisting runs before the live desktop settings are
/// applied so a repair is never lost to a window-state failure.
#[tauri::command]
pub async fn repair_app_settings(app: tauri::AppHandle) -> Result<AppSettings, CommandError> {
    let app_for_apply = app.clone();
    let repaired = observe(RuntimeLogAction::AppSettingsRepaired, async move {
        let state = state(&app)?;
        blocking(move || {
            state
                .repair_app_settings()
                .map_err(|error| CommandError::new("app-settings-repair-failed", error))
        })
        .await
    })
    .await?;
    apply_desktop_settings(&app_for_apply, &repaired)?;
    crate::runtime_log::set_level(repaired.runtime_log_level);
    crate::tray::refresh(&app_for_apply);
    Ok(repaired)
}

/// Installed system font families, offered by the interface-font picker.
#[tauri::command]
pub async fn list_system_fonts() -> Result<Vec<String>, CommandError> {
    blocking(|| Ok(crate::fonts::system_font_families())).await
}
