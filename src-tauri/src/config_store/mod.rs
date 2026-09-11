//! The application configuration store: providers, client settings, and
//! write history under `state/configuration/`.
//!
//! Layout (the only persisted shape):
//!
//! ```text
//! state/
//! ├─ configuration/
//! │  ├─ client-settings/{codex,claude}.json
//! │  ├─ providers/{codex,claude}/{uuid}.json
//! │  ├─ history/{codex,claude}.json
//! │  └─ save-journal.json (only while a confirmed active profile is applying)
//! └─ settings.json (app-runtime preferences, owned elsewhere)
//! ```
//!
//! Every mutation validates the typed contract before anything is written,
//! and every file lands through a temporary write plus atomic rename. None
//! of these files is a Codex or Claude Code configuration: switching remains
//! the only writer of real client files.

pub mod client_settings;
mod codex_providers;
pub mod history;
pub mod migration;
pub mod providers;
pub mod snapshot;

pub(crate) const SWITCH_INTENT_FILE: &str = "switch-intent.json";
pub(crate) const PROFILE_PREIMAGE_FILE: &str = "save-before.json";
pub(crate) const PROVIDER_POSITION_STEP: u64 = 100;

use asb_core::contracts::AppKind;
use asb_switch::io::{FsIo, SwitchIo};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
/// Read-side failures of the configuration store.
#[derive(Debug, Clone, PartialEq)]
pub enum ProfileStoreError {
    Unreadable,
    Unsupported,
}

impl std::fmt::Display for ProfileStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable => formatter.write_str("配置存储不可读"),
            Self::Unsupported => formatter
                .write_str("配置存储格式无效或来自已不受支持的旧版本；请重置或重新创建供应商数据"),
        }
    }
}

/// The one configuration store for one app-data directory.
pub struct ConfigStore {
    /// The `state/` directory itself.
    state_root: PathBuf,
}

/// Durable marker for an explicitly confirmed active-profile update. It has
/// no draft or credential material: the new provider file is the source used
/// to finish recovery after a process interruption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingProfileSave {
    pub profile_id: String,
    pub app: AppKind,
    pub previous_file_hash: String,
}

impl ConfigStore {
    pub fn new(state_root: PathBuf) -> Self {
        Self { state_root }
    }

    fn profile_save_journal_path(&self) -> PathBuf {
        self.configuration_dir().join("save-journal.json")
    }

    /// Reads the one pending active-profile save. An unreadable marker blocks
    /// subsequent writes; silently starting another transaction would make
    /// the recovery target ambiguous.
    pub fn pending_profile_save(&self) -> Result<Option<PendingProfileSave>, ProfileStoreError> {
        self.ensure_layout()?;
        match read_optional(&self.profile_save_journal_path())? {
            None => Ok(None),
            Some(text) => parse_strict(&text).map(Some),
        }
    }

    pub fn begin_profile_save(&self, pending: &PendingProfileSave) -> Result<(), String> {
        if self
            .pending_profile_save()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err("存在未完成的供应商保存，必须先完成恢复".to_string());
        }
        let json = serde_json::to_string_pretty(pending)
            .map_err(|_| "供应商保存恢复记录序列化失败".to_string())?;
        write_json_atomic(&self.profile_save_journal_path(), &json)
    }

    pub fn clear_profile_save(&self) -> Result<(), String> {
        match fs::remove_file(self.profile_save_journal_path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("无法清除供应商保存恢复记录".to_string()),
        }
    }

    pub fn configuration_dir(&self) -> PathBuf {
        self.state_root.join("configuration")
    }

    pub fn legacy_store_path(&self) -> PathBuf {
        self.state_root.join("profiles.json")
    }

    fn client_dir(&self, kind: &str) -> PathBuf {
        self.configuration_dir().join(kind)
    }

    pub fn client_settings_path(&self, app: AppKind) -> PathBuf {
        self.client_dir("client-settings")
            .join(format!("{}.json", app.dir_name()))
    }

    /// The directory marks an initialized current layout even when neither
    /// client has saved preferences yet. Missing provider fields in that
    /// layout are corruption, never evidence of a predecessor schema.
    fn initialize_current_layout(&self) -> Result<(), String> {
        self.ensure_layout().map_err(|error| error.to_string())?;
        fs::create_dir_all(self.client_dir("client-settings"))
            .map_err(|error| format!("无法初始化客户端设置目录：{error}"))
    }

    pub fn providers_dir(&self, app: AppKind) -> PathBuf {
        self.client_dir("providers").join(app.dir_name())
    }

    pub fn history_path(&self, app: AppKind) -> PathBuf {
        self.client_dir("history")
            .join(format!("{}.json", app.dir_name()))
    }

    /// Runtime readers never interpret predecessor data. Startup completes
    /// the offline conversion before any command can observe this store.
    pub fn ensure_layout(&self) -> Result<(), ProfileStoreError> {
        if self.legacy_store_path().exists()
            || self.configuration_dir().join("common").exists()
            || migration::journal_path(self).exists()
            || (self.client_dir("client-settings").exists()
                && !self.client_dir("client-settings").is_dir())
        {
            return Err(ProfileStoreError::Unsupported);
        }
        Ok(())
    }

    /// Removes every persisted provider, client setting, and history record.
    /// The reset is the recovery path for unreadable legacy data.
    pub fn reset(&self) -> Result<(), String> {
        if let Err(error) = fs::remove_dir_all(self.configuration_dir()) {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err("无法删除配置存储目录".to_string());
            }
        }
        if let Err(error) = fs::remove_file(self.legacy_store_path()) {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err("无法删除旧版配置数据".to_string());
            }
        }
        Ok(())
    }
}

/// Deterministic content revision for optimistic saves. Like the previous
/// base revision it is a content fingerprint, not a security primitive.
pub(crate) fn content_revision(bytes: &[u8]) -> String {
    let mut value = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{value:016x}")
}

/// Writes one JSON file through a sibling temporary file and an atomic
/// rename, creating parent directories on demand.
pub(crate) fn write_json_atomic(path: &Path, json: &str) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "无效的存储路径".to_string())?;
    fs::create_dir_all(parent).map_err(|_| "无法创建配置存储目录".to_string())?;
    let temporary = parent.join(format!(
        "{}.{}.tmp",
        safe_file_stem(path),
        Uuid::new_v4().simple()
    ));
    if let Err(error) = FsIo
        .write_new_file(&temporary, json)
        .and_then(|_| FsIo.sync_file(&temporary))
    {
        let cleanup = fs::remove_file(&temporary);
        return Err(format!(
            "无法写入并同步配置存储临时文件：{error}{}",
            if cleanup.is_err() && temporary.exists() {
                "；临时文件清理失败"
            } else {
                ""
            }
        ));
    }
    if fs::rename(&temporary, path).is_err() {
        let _ = fs::remove_file(&temporary);
        return Err("无法原子保存配置存储".to_string());
    }
    FsIo.sync_dir(parent)
        .map_err(|_| "配置已替换，但存储目录同步失败".to_string())
}

fn safe_file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "store".to_string())
}

/// Reads one JSON file, mapping absence to `None`.
pub(crate) fn read_optional(path: &Path) -> Result<Option<String>, ProfileStoreError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(ProfileStoreError::Unreadable),
    }
}

/// Strictly parses one JSON document; any shape drift is unsupported.
pub(crate) fn parse_strict<T: serde::de::DeserializeOwned>(
    text: &str,
) -> Result<T, ProfileStoreError> {
    serde_json::from_str(text).map_err(|_| ProfileStoreError::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_revision_is_stable_and_order_sensitive() {
        let bytes = b"model_reasoning_effort: high";
        assert_eq!(content_revision(bytes), content_revision(bytes));
        assert_ne!(content_revision(bytes), content_revision(b"other"));
    }

    #[test]
    fn reset_removes_current_layout_and_legacy_file_without_requiring_them() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = ConfigStore::new(directory.path().join("state"));
        fs::create_dir_all(store.providers_dir(AppKind::Codex)).expect("provider dir");
        fs::write(store.legacy_store_path(), b"{}").expect("legacy file");

        store.reset().expect("reset");

        assert!(!store.configuration_dir().exists());
        assert!(!store.legacy_store_path().exists());
        store.reset().expect("reset of an absent layout is fine");
    }

    #[test]
    fn ensure_layout_accepts_a_clean_state_and_rejects_both_present() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = ConfigStore::new(directory.path().join("state"));
        store
            .ensure_layout()
            .expect("clean state is immediately usable");

        fs::create_dir_all(store.configuration_dir()).expect("configuration dir");
        fs::write(store.legacy_store_path(), b"{}").expect("legacy file");
        assert_eq!(
            store.ensure_layout().expect_err("both present must fail"),
            ProfileStoreError::Unsupported
        );
    }

    #[test]
    fn pending_profile_save_is_strict_and_can_be_cleared() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = ConfigStore::new(directory.path().join("state"));
        let pending = PendingProfileSave {
            profile_id: "profile-id".to_string(),
            app: AppKind::Codex,
            previous_file_hash: "previous-hash".to_string(),
        };

        assert_eq!(store.pending_profile_save().unwrap(), None);
        store.begin_profile_save(&pending).expect("write marker");
        assert_eq!(store.pending_profile_save().unwrap(), Some(pending.clone()));
        assert!(store.begin_profile_save(&pending).is_err());

        store.clear_profile_save().expect("clear marker");
        assert_eq!(store.pending_profile_save().unwrap(), None);
        store
            .clear_profile_save()
            .expect("clearing absence is idempotent");
    }
}
