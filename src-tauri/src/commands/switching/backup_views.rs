use super::{backups::find_backup, client_settings_backup};
use crate::commands::error::{blocking, state, CommandError};
use asb_core::{adapter, AppKind, KeyChange};
use asb_switch::{sha256_hex, FsIo, SwitchIo};
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// Complete file and saved preference difference from one backup. `before` is
/// the backup value, `after` the current one.
#[tauri::command]
pub async fn backup_diff(
    app: AppHandle,
    backup_id: String,
) -> Result<Vec<KeyChange>, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let record = find_backup(&state, &backup_id)?;
        let target = state
            .target(record.app)
            .map_err(|error| CommandError::new("config-path-unavailable", error))?;
        let current = match FsIo.read_file(&target) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound =>
                if record.app == AppKind::Claude { "{}".into() } else { String::new() },
            Err(_) => return Err(CommandError::keyed(
                "read-current",
                "errors.sw.currentConfigUnreadableDiffUnavailable",
                "无法读取当前配置文件，无法生成差异",
            )),
        };
        let backup_text = FsIo
            .read_file(PathBuf::from(&record.backup_path).as_path())
            .map_err(|_| {
                CommandError::keyed(
                    "backup-unreadable",
                    "errors.sw.backupUnreadableDiffUnavailable",
                    "备份文件不可读，无法生成差异",
                )
            })?;
        if sha256_hex(&backup_text) != record.content_hash {
            return Err(CommandError::keyed(
                "backup-hash-mismatch",
                "errors.sw.backupHashMismatchDiffRefused",
                "备份内容与记录哈希不符，拒绝生成差异",
            ));
        }
        let mut changes = adapter::full_diff(record.app, &current, &backup_text)
            .map_err(|error| CommandError::new("diff-failed", error.to_string()))?;
        changes.extend(client_settings_backup::diff(&state, &record)
            .map_err(|error| CommandError::new("backup-settings-invalid", error))?);
        Ok(changes)
    })
    .await
}

/// Opens the app-owned backup directory in the system file manager. The
/// directory is created on demand so an empty history still opens cleanly.
#[tauri::command]
pub async fn open_backup_dir(app: AppHandle) -> Result<(), CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let dir = state.backup_dir();
        std::fs::create_dir_all(&dir).map_err(|_| {
            CommandError::keyed(
                "backup-dir-unavailable",
                "errors.sw.backupDirCreateFailed",
                "无法创建备份目录",
            )
        })?;
        app.opener()
            .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
            .map_err(|_| {
                CommandError::keyed(
                    "backup-dir-open-failed",
                    "errors.sw.backupDirOpenFailed",
                    "无法打开备份文件夹",
                )
            })
    })
    .await
}
