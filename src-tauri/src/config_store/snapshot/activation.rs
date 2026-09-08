use super::super::{history, write_json_atomic, ConfigStore};
use super::{read_configuration_snapshot, validate_snapshot, ConfigurationSnapshot};
use asb_core::contracts::{AppKind, ConfigWriteRecord};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn snapshot_history_file(records: &[ConfigWriteRecord]) -> Result<String, String> {
    serde_json::to_string_pretty(&history::HistoryFile {
        records: records.to_vec(),
    })
    .map_err(|_| "写入历史序列化失败".to_string())
}

/// Writes a validated snapshot into `target`, which must not exist yet, then
/// reads every file back and checks the result equals the snapshot.
pub(crate) fn stage_and_verify(
    target: &Path,
    snapshot: &ConfigurationSnapshot,
) -> Result<(), String> {
    validate_snapshot(snapshot)?;
    for app in [AppKind::Codex, AppKind::Claude] {
        for file in &snapshot.providers[&app] {
            let json = serde_json::to_string_pretty(file)
                .map_err(|_| "供应商文件序列化失败".to_string())?;
            write_json_atomic(
                &target
                    .join("providers")
                    .join(app.dir_name())
                    .join(format!("{}.json", file.id)),
                &json,
            )?;
        }
        let settings_json = serde_json::to_string_pretty(&snapshot.client_settings[&app])
            .map_err(|_| "客户端设置序列化失败".to_string())?;
        write_json_atomic(
            &target
                .join("client-settings")
                .join(format!("{}.json", app.dir_name())),
            &settings_json,
        )?;
        let history_json = snapshot_history_file(&snapshot.history[&app])?;
        write_json_atomic(
            &target
                .join("history")
                .join(format!("{}.json", app.dir_name())),
            &history_json,
        )?;
    }

    // Read back through the strict runtime readers and compare. The staged
    // directory is itself a configuration directory, so its parent is the
    // probe's state root.
    let probe = ConfigStore::new(
        target
            .parent()
            .ok_or_else(|| "无效的暂存路径".to_string())?
            .to_path_buf(),
    );
    let read_back = read_configuration_snapshot(&probe).map_err(|error| error.to_string())?;
    if read_back != *snapshot {
        return Err("暂存配置回读校验不一致".to_string());
    }
    Ok(())
}

pub(crate) enum Activation {
    Enabled { retired: Option<PathBuf> },
    Restored(String),
    RecoveryRequired(String),
}

/// Activates a staged directory without deleting either side before the live
/// directory has been restored or replaced. The injectable rename operation
/// keeps the failure boundary testable without a second filesystem contract.
pub(crate) fn activate_staged<F>(
    live: &Path,
    staged: &Path,
    retired: &Path,
    rename: &mut F,
) -> Activation
where
    F: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    if !live.exists() {
        return match rename(staged, live) {
            Ok(()) => Activation::Enabled { retired: None },
            Err(error) => Activation::Restored(format!("无法启用新配置目录：{error}")),
        };
    }

    if let Err(error) = rename(live, retired) {
        return Activation::Restored(format!("无法移出旧配置目录：{error}"));
    }
    match rename(staged, live) {
        Ok(()) => Activation::Enabled {
            retired: Some(retired.to_path_buf()),
        },
        Err(error) => match rename(retired, live) {
            Ok(()) => Activation::Restored(format!(
                "无法启用新配置目录：{error}；已恢复原配置目录"
            )),
            Err(restore_error) => Activation::RecoveryRequired(format!(
                "无法启用新配置目录：{error}；自动恢复也失败：{restore_error}。原配置保留在 {}，新暂存配置保留在 {}",
                retired.display(),
                staged.display()
            )),
        },
    }
}

/// Replaces the live `configuration` directory with a validated snapshot.
/// The snapshot is written into a private staging root, read back through
/// the strict runtime readers, and only then swapped into place. A failed
/// swap restores the old directory; an unrecoverable filesystem failure keeps
/// both directories in place and returns their exact recovery paths.
pub fn enable_snapshot(
    store: &ConfigStore,
    snapshot: &ConfigurationSnapshot,
) -> Result<(), String> {
    let staging_root = store
        .state_root
        .join(format!("staging-{}", Uuid::new_v4().simple()));
    let staged = staging_root.join("configuration");
    let mut preserve_staging = false;
    let result = stage_and_verify(&staged, snapshot).and_then(|()| {
        let live = store.configuration_dir();
        let retired = store
            .state_root
            .join(format!("retired-{}", Uuid::new_v4().simple()));
        let mut rename = |from: &Path, to: &Path| fs::rename(from, to);
        match activate_staged(&live, &staged, &retired, &mut rename) {
            Activation::Enabled { retired } => {
                if let Some(retired) = retired {
                    let _ = fs::remove_dir_all(retired);
                }
                Ok(())
            }
            Activation::Restored(error) => Err(error),
            Activation::RecoveryRequired(error) => {
                preserve_staging = true;
                Err(error)
            }
        }
    });
    if !preserve_staging {
        let _ = fs::remove_dir_all(&staging_root);
    }
    result
}
