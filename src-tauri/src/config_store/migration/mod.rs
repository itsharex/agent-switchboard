//! Clean-break boundary for retired configuration layouts.
//!
//! Current configuration is never upgraded in place: deprecated profiles and
//! transaction journals must be reset or recovered by the user before this
//! version can run.

use super::ConfigStore;
use std::path::PathBuf;

pub(super) fn journal_path(store: &ConfigStore) -> PathBuf {
    store.state_root.join("configuration-upgrade.json")
}

impl ConfigStore {
    pub(crate) fn upgrade_if_needed(&self) -> Result<bool, String> {
        if self.legacy_store_path().exists()
            || self.configuration_dir().join("common").exists()
            || journal_path(self).exists()
        {
            return Err("检测到已不受支持的旧配置格式；请重置或重新创建供应商数据".to_string());
        }
        self.initialize_current_layout()?;
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn retired_upgrade_journal_is_rejected_without_rewriting_it() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = ConfigStore::new(directory.path().join("state"));
        let journal = journal_path(&store);
        fs::create_dir_all(journal.parent().expect("configuration directory"))
            .expect("create configuration directory");
        fs::write(&journal, "retired migration state").expect("write retired journal");

        assert!(store.upgrade_if_needed().is_err());
        assert_eq!(
            fs::read_to_string(journal).unwrap(),
            "retired migration state"
        );
    }
}
