//! One startup-only conversion of the directly preceding configuration.
//! Conversion is verified in isolation before the whole directory is swapped;
//! a durable journal makes either rename recoverable after interruption.

mod layout;
mod recovery;
pub(crate) mod responses;
#[cfg(test)]
mod responses_tests;
mod subagent_parameters;
#[cfg(test)]
mod tests;

use super::snapshot::activation::{activate_staged, stage_and_verify, Activation};
use super::{write_json_atomic, ConfigStore};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

static UPGRADE_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn journal_path(store: &ConfigStore) -> PathBuf {
    store.state_root.join("configuration-upgrade.json")
}

impl ConfigStore {
    pub(crate) fn upgrade_if_needed(&self) -> Result<bool, String> {
        let _guard = UPGRADE_LOCK
            .lock()
            .map_err(|_| "配置升级锁异常".to_string())?;
        recovery::recover(self)?;
        if self.legacy_store_path().exists() {
            return Err("不支持聚合供应商旧格式，配置升级未写入任何数据".to_string());
        }
        if !layout::needs_upgrade(self)?
            && !responses::needs_upgrade(self)?
            && !subagent_parameters::needs_upgrade(self)?
        {
            self.initialize_current_layout()?;
            return Ok(false);
        }
        upgrade(self)?;
        Ok(true)
    }
}

fn upgrade(store: &ConfigStore) -> Result<(), String> {
    for name in [super::SWITCH_INTENT_FILE, super::PROFILE_PREIMAGE_FILE] {
        let path = store.configuration_dir().join(name);
        if path.exists() {
            return Err(format!(
                "配置存在已确认但未完成的事务，不能升级并改变其版本；请先恢复 {}，原目录保持不变",
                path.display()
            ));
        }
    }
    let original = layout::fingerprint(&store.configuration_dir())?;
    let snapshot = if layout::needs_upgrade(store)? {
        layout::read_previous(store)?
    } else {
        responses::read_previous(store)?
    };
    let pending = layout::pending_save(store)?;
    let journal = recovery::UpgradeJournal::new();
    let (staging, retired) = journal.paths(store)?;
    let staged = staging.join("configuration");
    let prepared = stage_and_verify(&staged, &snapshot).and_then(|()| {
        if let Some(pending) = &pending {
            write_json_atomic(
                &staged.join("save-journal.json"),
                &serde_json::to_string_pretty(pending).map_err(|error| error.to_string())?,
            )?;
        }
        if layout::fingerprint(&store.configuration_dir())? != original {
            return Err("配置在升级暂存期间已改变，原目录保持不变，请重新启动".to_string());
        }
        write_json_atomic(
            &journal_path(store),
            &serde_json::to_string_pretty(&journal).map_err(|error| error.to_string())?,
        )
    });
    if let Err(error) = prepared {
        return Err(match recovery::remove_directory(&staging) {
            Ok(()) => error,
            Err(cleanup) => format!("{error}；{cleanup}"),
        });
    }
    let mut rename = |from: &std::path::Path, to: &std::path::Path| fs::rename(from, to);
    match activate_staged(&store.configuration_dir(), &staged, &retired, &mut rename) {
        Activation::Enabled { .. } => recovery::finish(store, &staging, &retired),
        Activation::Restored(error) => {
            let cleanup =
                recovery::remove_directory(&staging).and_then(|()| recovery::remove_journal(store));
            Err(match cleanup {
                Ok(()) => error,
                Err(cleanup) => format!("{error}；{cleanup}"),
            })
        }
        Activation::RecoveryRequired(error) => Err(error),
    }
}
