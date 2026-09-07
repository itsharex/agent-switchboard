//! The extension library: the single persistence root for extension
//! definitions, bindings, baselines, sources, projects, immutable skill
//! content versions, operation history, and check summaries.

//! Layout (inside `<app-data>/state/extensions/`):

//! ```text
//! manifest.json                 schemaVersion + generation
//! definitions/<id>.json
//! bindings/<id>.json
//! projects/<id>.json
//! baselines/<binding-id>.json   restricted local metadata, never sent to UI
//! library/<resource-id>/<digest>/ + .asb-content.json
//! transactions/<operation-id>/  journal (owned by the switch executor)
//! history/<operation-id>.json   redacted, displayable operation records
//! checks/<definition-id>.json   latest check per definition
//! ../backups/extensions/<operation-id>/  protected client-file backups
//! ```

//! Everything is written atomically and parsed strictly; there is no
//! migration path from foreign shapes.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::OnceLock;

use asb_core::extensions::contracts::{
    ExtensionBinding, ExtensionManifest, ManagedBaselineFile, OperationSnapshot,
};

use crate::config_store::write_json_atomic;

/// One operation's complete library change, journaled by the switch
/// executor before the commit runs and replayed backwards by
/// [`ExtensionStore::recover_library`] after a crash. The pre-state makes
/// restoration deterministic; nothing is inferred from current files.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryCommit {
    /// The history entry this batch may create. It is journaled even before
    /// the record exists so crash recovery can remove a half-written
    /// snapshot without guessing.
    pub history_operation_id: String,
    pub binding_upserts: Vec<ExtensionBinding>,
    pub baseline_files: Vec<(String, ManagedBaselineFile)>,
    /// Existing bindings whose baseline is removed without deleting the
    /// binding itself (for example while restoring a first application).
    pub baseline_deletes: Vec<String>,
    pub binding_deletes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<OperationSnapshot>,
    /// Library state before the operation.
    #[serde(default)]
    pub pre_bindings: Vec<ExtensionBinding>,
    #[serde(default)]
    pub pre_baselines: Vec<(String, ManagedBaselineFile)>,
}

/// Durable proof that a library batch reached its final commit point. It is
/// written only after every library record and the generation marker have
/// landed. If a process dies before the executor can append `Committed` to
/// its journal, this receipt prevents recovery from undoing a completed
/// client + library operation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LibraryCommitMarker {
    schema_version: u8,
    operation_id: String,
}

/// Serializes all library mutations inside this process. Cross-process
/// safety comes from the atomic-write discipline; the in-process lock keeps
/// generation bookkeeping coherent.
static LIBRARY_SAVE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn library_lock() -> &'static Mutex<()> {
    LIBRARY_SAVE_LOCK.get_or_init(|| Mutex::new(()))
}

/// One immutable skill content version stored under
/// `library/<resource-id>/<digest>/`.

#[derive(Clone)]
pub struct ExtensionStore {
    root: PathBuf,
    extension_backup_root: PathBuf,
}

impl ExtensionStore {
    pub fn from_state(state: &crate::local_state::LocalState) -> Self {
        Self::open(
            state.root().join("extensions"),
            state.backup_dir().join("extensions"),
        )
    }

    #[cfg(test)]
    pub(crate) fn from_root(root: PathBuf) -> Self {
        let extension_backup_root = root.join("backups").join("extensions");
        Self::open(root, extension_backup_root)
    }

    /// Constructs a store for a library that [`LocalState`] has already
    /// admitted through its startup schema gate. Migration has exactly one
    /// owner there; opening a store must not perform a second hidden write or
    /// turn a recoverable migration error into a process panic.
    pub(super) fn open(root: PathBuf, extension_backup_root: PathBuf) -> Self {
        Self {
            root,
            extension_backup_root,
        }
    }

    pub(super) fn path(&self, relative: &[&str]) -> PathBuf {
        let mut path = self.root.clone();
        for segment in relative {
            path.push(segment);
        }
        path
    }

    pub(super) fn read_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &Path,
    ) -> Result<T, ExtensionStoreError> {
        let text = fs::read_to_string(path).map_err(|error| {
            ExtensionStoreError::Unreadable(format!("{}：{error}", path.display()))
        })?;
        serde_json::from_str(&text).map_err(|error| {
            ExtensionStoreError::Unsupported(format!("{}：{error}", path.display()))
        })
    }

    pub(super) fn write_json<T: serde::Serialize>(
        &self,
        path: &Path,
        value: &T,
    ) -> Result<(), ExtensionStoreError> {
        let json = serde_json::to_string_pretty(value)
            .map_err(|error| ExtensionStoreError::Unreadable(error.to_string()))?;
        write_json_atomic(path, &json).map_err(ExtensionStoreError::Unreadable)
    }

    /// Captures the only mutable commit marker before a metadata mutation.
    /// It is restored alongside the changed file when advancing the marker
    /// fails, so a failed create/update/delete never leaves an unannounced
    /// library state behind.
    pub(super) fn manifest_state_locked(
        &self,
    ) -> Result<Option<ExtensionManifest>, ExtensionStoreError> {
        let path = self.path(&["manifest.json"]);
        if path.exists() {
            Ok(Some(self.read_json(&path)?))
        } else {
            Ok(None)
        }
    }

    pub(super) fn restore_manifest_state_locked(
        &self,
        previous: Option<&ExtensionManifest>,
    ) -> Result<(), ExtensionStoreError> {
        let path = self.path(&["manifest.json"]);
        match previous {
            Some(manifest) => self.write_json(&path, manifest),
            None => self.remove_optional_file(&path),
        }
    }

    pub(super) fn restore_file_and_manifest_locked<T: serde::Serialize>(
        &self,
        path: &Path,
        previous_file: Option<&T>,
        previous_manifest: Option<&ExtensionManifest>,
    ) -> Result<(), ExtensionStoreError> {
        let mut failures = Vec::new();
        let file_result = match previous_file {
            Some(value) => self.write_json(path, value),
            None => self.remove_optional_file(path),
        };
        if let Err(error) = file_result {
            failures.push(error.to_string());
        }
        if let Err(error) = self.restore_manifest_state_locked(previous_manifest) {
            failures.push(error.to_string());
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(ExtensionStoreError::Unreadable(failures.join("；")))
        }
    }

    pub(super) fn metadata_failure<T>(
        &self,
        operation: &str,
        error: ExtensionStoreError,
        recovery: Result<(), ExtensionStoreError>,
    ) -> Result<T, ExtensionStoreError> {
        match recovery {
            Ok(()) => Err(error),
            Err(recovery) => Err(ExtensionStoreError::RecoveryRequired(format!(
                "{operation}失败：{error}；恢复修改前状态也失败：{recovery}"
            ))),
        }
    }
}

mod commit;
mod content;
mod records;
#[cfg(test)]
mod tests;

pub use content::ExtensionStoreError;
