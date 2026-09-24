//! Application-owned persistent directories. Windows Known Folder APIs ignore
//! process APPDATA/LOCALAPPDATA overrides, so honor those before OS defaults.

use std::path::PathBuf;

pub(crate) fn data_directory(identifier: &str) -> Result<PathBuf, String> {
    #[cfg(windows)]
    let root = windows_directory("APPDATA", dirs::data_dir)?;
    #[cfg(not(windows))]
    let root = dirs::data_dir().ok_or("无法定位应用数据目录")?;
    Ok(root.join(identifier))
}

pub(crate) fn local_data_directory(identifier: &str) -> Result<PathBuf, String> {
    #[cfg(windows)]
    let root = windows_directory("LOCALAPPDATA", dirs::data_local_dir)?;
    #[cfg(not(windows))]
    let root = dirs::data_local_dir().ok_or("无法定位应用本地数据目录")?;
    Ok(root.join(identifier))
}

pub(crate) fn log_directory(identifier: &str) -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    return dirs::home_dir()
        .map(|home| home.join("Library/Logs").join(identifier))
        .ok_or_else(|| "无法定位应用日志目录".to_string());
    #[cfg(not(target_os = "macos"))]
    Ok(local_data_directory(identifier)?.join("logs"))
}

/// Tauri's config accepts only relative WebView paths and resolves them via
/// Known Folders. Defer the same windows so the builder can receive the
/// absolute process-selected path before Tauri touches a data directory.
#[cfg(windows)]
pub(crate) fn prepare_windows<R: tauri::Runtime>(
    context: &mut tauri::Context<R>,
) -> Vec<tauri::utils::config::WindowConfig> {
    let mut windows = Vec::new();
    for config in &mut context.config_mut().app.windows {
        if config.create {
            windows.push(config.clone());
            config.create = false;
        }
    }
    windows
}

#[cfg(windows)]
pub(crate) fn build_windows(
    app: &tauri::AppHandle,
    windows: &[tauri::utils::config::WindowConfig],
) -> Result<(), String> {
    let directory = local_data_directory(&app.config().identifier)?;
    let mut windows = windows.to_vec();
    crate::apply_startup_hardware_acceleration(app, &mut windows);
    for config in &windows {
        tauri::WebviewWindowBuilder::from_config(app, config)
            .map_err(|error| error.to_string())?
            .data_directory(directory.clone())
            .build()
            .map_err(|error| format!("窗口创建失败: {error}"))?;
    }
    Ok(())
}

#[cfg(windows)]
fn windows_directory(
    key: &str,
    fallback: impl FnOnce() -> Option<PathBuf>,
) -> Result<PathBuf, String> {
    let value = std::env::var_os(key);
    resolve_windows_directory(key, value.as_deref(), fallback)
}

#[cfg(windows)]
fn resolve_windows_directory(
    key: &str,
    value: Option<&std::ffi::OsStr>,
    fallback: impl FnOnce() -> Option<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            return Err(format!("{key} 必须是绝对目录"));
        }
        return Ok(path);
    }
    fallback().ok_or_else(|| format!("无法定位 {key} 目录"))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn explicit_windows_directory_never_consults_the_real_profile() {
        let path = PathBuf::from(r"F:\isolated\appdata");
        let resolved = resolve_windows_directory("APPDATA", Some(path.as_os_str()), || {
            panic!("an explicit override must not consult the OS profile")
        })
        .unwrap();
        assert_eq!(resolved, path);
    }

    #[test]
    fn relative_override_fails_instead_of_falling_back_to_real_data() {
        let result =
            resolve_windows_directory("LOCALAPPDATA", Some(OsStr::new("relative")), || {
                panic!("an invalid override must not fall back to the OS profile")
            });
        assert_eq!(result.unwrap_err(), "LOCALAPPDATA 必须是绝对目录");
    }

    #[test]
    fn absent_or_empty_override_preserves_the_platform_default() {
        let default = PathBuf::from(r"C:\Users\Default\AppData\Roaming");
        for value in [None, Some(OsStr::new(""))] {
            let resolved = resolve_windows_directory("APPDATA", value, || Some(default.clone()));
            assert_eq!(resolved.unwrap(), default);
        }
    }
}
