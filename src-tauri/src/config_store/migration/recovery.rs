use super::{journal_path, layout, ConfigStore};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UpgradeJournal {
    id: String,
}

impl UpgradeJournal {
    pub(super) fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
        }
    }

    pub(super) fn paths(&self, store: &ConfigStore) -> Result<(PathBuf, PathBuf), String> {
        let parsed =
            uuid::Uuid::parse_str(&self.id).map_err(|_| "配置升级日志标识无效".to_string())?;
        if parsed.to_string() != self.id {
            return Err("配置升级日志标识格式无效".to_string());
        }
        Ok((
            store
                .state_root
                .join(format!("configuration-upgrade-{}", self.id)),
            store
                .state_root
                .join(format!("configuration-before-upgrade-{}", self.id)),
        ))
    }
}

pub(super) fn remove_directory(path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法清理配置升级目录 {}：{error}", path.display())),
    }
}

pub(super) fn remove_journal(store: &ConfigStore) -> Result<(), String> {
    fs::remove_file(journal_path(store)).map_err(|error| format!("无法清理配置升级日志：{error}"))
}

pub(super) fn finish(store: &ConfigStore, staging: &Path, retired: &Path) -> Result<(), String> {
    remove_directory(staging)?;
    if retired.exists() {
        log::info!(
            "配置升级已完成，升级前备份保留在 {}；Codex 第三方统一使用本机网关 HTTP/SSE",
            retired.display()
        );
    }
    remove_journal(store)
}

/// If the new directory reached the live location, validate it before
/// releasing the pre-image. Otherwise restore the old directory and retry
/// conversion; no caller can observe a missing or mixed configuration.
pub(super) fn recover(store: &ConfigStore) -> Result<(), String> {
    let Some(journal) = layout::read_json::<UpgradeJournal>(&journal_path(store))? else {
        return Ok(());
    };
    let (staging, retired) = journal.paths(store)?;
    let staged = staging.join("configuration");
    let live = store.configuration_dir();
    match (live.exists(), retired.exists(), staged.exists()) {
        (true, true, false) => {
            layout::validate_current(store)?;
            finish(store, &staging, &retired)
        }
        (true, false, false) => {
            if layout::needs_upgrade(store)? || super::responses::needs_upgrade(store)? {
                remove_directory(&staging)?;
                remove_journal(store)
            } else {
                layout::validate_current(store)?;
                finish(store, &staging, &retired)
            }
        }
        (false, true, true) => {
            fs::rename(&retired, &live).map_err(|error| {
                format!(
                    "无法恢复升级前配置 {} 到 {}：{error}",
                    retired.display(),
                    live.display()
                )
            })?;
            remove_directory(&staging)?;
            remove_journal(store)
        }
        (true, false, true) => {
            remove_directory(&staging)?;
            remove_journal(store)
        }
        _ => Err(format!(
            "配置升级恢复状态不完整，请保留并检查 {}、{} 和 {}",
            live.display(),
            retired.display(),
            staged.display()
        )),
    }
}
