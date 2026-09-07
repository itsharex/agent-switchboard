//! Library commits: one locked batch that writes bindings, baselines,
//! the history snapshot, and the generation marker together — or restores
//! the exact pre-state.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use asb_core::extensions::contracts::{
    ExtensionBinding, ExtensionManifest, ManagedBaselineFile, EXTENSIONS_SCHEMA_VERSION,
};

use super::*;

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

impl ExtensionStore {
    /// Takes the process-wide save lock and commits one batch without any
    /// currency precondition (recovery uses this).
    #[cfg(test)]
    pub fn commit_batch(&self, commit: &LibraryCommit) -> Result<(), ExtensionStoreError> {
        let _guard = save_lock();
        self.commit_batch_locked(commit)
    }

    /// Same as [`Self::commit_batch`], but also verifies the exact library
    /// state used to prepare the plan while holding the mutation lock. The
    /// executor calls this after client writes; a stale definition or
    /// generation makes it roll the client writes back rather than commit a
    /// plan the user did not preview.
    pub fn commit_batch_if_current(
        &self,
        commit: &LibraryCommit,
        expected_generation: u64,
        expected_revisions: &[(String, u64)],
    ) -> Result<(), ExtensionStoreError> {
        let _guard = save_lock();
        self.ensure_generation_locked(expected_generation)?;
        // Every resource the plan was prepared against must still be at the
        // prepared revision; a change on any one of them re-previews the
        // whole batch.
        for (definition_id, expected_definition_revision) in expected_revisions {
            let definition = self.get_definition(definition_id)?.ok_or_else(|| {
                ExtensionStoreError::Conflict("扩展已被删除；请重新预览".to_string())
            })?;
            if definition.revision != *expected_definition_revision {
                return Err(ExtensionStoreError::Conflict(
                    "扩展定义已变化；请重新预览后应用".to_string(),
                ));
            }
        }
        self.commit_batch_locked(commit)
    }

    /// Commits an historical restore after checking the library generation
    /// captured by its restore plan. A restore is intentionally independent
    /// of the original definition: that definition may have been removed by
    /// the operation now being reversed.
    pub fn commit_restore_if_current(
        &self,
        commit: &LibraryCommit,
        expected_generation: u64,
    ) -> Result<(), ExtensionStoreError> {
        let _guard = save_lock();
        self.ensure_generation_locked(expected_generation)?;
        self.commit_batch_locked(commit)
    }

    pub(super) fn ensure_generation_locked(
        &self,
        expected_generation: u64,
    ) -> Result<(), ExtensionStoreError> {
        if self.manifest()?.generation != expected_generation {
            return Err(ExtensionStoreError::Conflict(
                "扩展库已发生变化；请重新预览后应用".to_string(),
            ));
        }
        Ok(())
    }

    pub(super) fn commit_batch_locked(
        &self,
        commit: &LibraryCommit,
    ) -> Result<(), ExtensionStoreError> {
        self.validate_commit(commit)?;
        if self.has_commit_marker(&commit.history_operation_id)? {
            return Err(ExtensionStoreError::Conflict(format!(
                "操作 {} 已完成扩展库提交",
                commit.history_operation_id
            )));
        }
        let manifest_before = self.manifest_state_locked()?;
        let result = (|| {
            for binding in &commit.binding_upserts {
                self.write_binding_locked(binding)?;
            }
            for (binding_id, baseline) in &commit.baseline_files {
                self.write_baseline_locked(binding_id, baseline)?;
            }
            for binding_id in &commit.baseline_deletes {
                self.remove_optional_file(
                    &self.path(&["baselines", &format!("{binding_id}.json")]),
                )?;
            }
            for binding_id in &commit.binding_deletes {
                self.remove_optional_file(
                    &self.path(&["bindings", &format!("{binding_id}.json")]),
                )?;
                self.remove_optional_file(
                    &self.path(&["baselines", &format!("{binding_id}.json")]),
                )?;
            }
            if let Some(snapshot) = &commit.snapshot {
                self.write_json(
                    &self.path(&["history", &format!("{}.json", snapshot.record.id)]),
                    snapshot,
                )?;
            }
            self.bump_generation_locked()
                .and_then(|()| self.write_commit_marker_locked(&commit.history_operation_id))
        })();
        if let Err(error) = result {
            if let Err(recovery) =
                self.restore_commit_state_locked(commit, manifest_before.as_ref())
            {
                return Err(ExtensionStoreError::RecoveryRequired(format!(
                    "扩展库提交失败：{error}；恢复提交前状态也失败：{recovery}"
                )));
            }
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn validate_commit(
        &self,
        commit: &LibraryCommit,
    ) -> Result<(), ExtensionStoreError> {
        if commit.history_operation_id.trim().is_empty() {
            return Err(ExtensionStoreError::Conflict(
                "扩展库提交缺少操作标识".to_string(),
            ));
        }
        if let Some(snapshot) = &commit.snapshot {
            if snapshot.record.id != commit.history_operation_id {
                return Err(ExtensionStoreError::Conflict(
                    "操作快照标识与提交标识不一致".to_string(),
                ));
            }
        }
        let mut upserts = BTreeSet::new();
        for binding in &commit.binding_upserts {
            super::records::require_current_schema("提交绑定", binding.schema_version)?;
            if !upserts.insert(binding.id.clone()) {
                return Err(ExtensionStoreError::Conflict(format!(
                    "提交中重复写入绑定 {}",
                    binding.id
                )));
            }
        }
        let mut baselines = BTreeSet::new();
        for (binding_id, baseline) in &commit.baseline_files {
            super::records::require_current_schema("提交基线", baseline.schema_version)?;
            if !baselines.insert(binding_id.clone()) {
                return Err(ExtensionStoreError::Conflict(format!(
                    "提交中重复写入基线 {binding_id}"
                )));
            }
            if baseline.binding_id != *binding_id {
                return Err(ExtensionStoreError::Conflict(format!(
                    "基线 {} 的绑定标识不一致",
                    binding_id
                )));
            }
        }
        let mut baseline_deletes = BTreeSet::new();
        for binding_id in &commit.baseline_deletes {
            if !baseline_deletes.insert(binding_id.clone()) {
                return Err(ExtensionStoreError::Conflict(format!(
                    "提交中重复删除基线 {binding_id}"
                )));
            }
            if baselines.contains(binding_id) {
                return Err(ExtensionStoreError::Conflict(format!(
                    "提交同时写入并删除基线 {binding_id}"
                )));
            }
        }
        let mut deletes = BTreeSet::new();
        for binding_id in &commit.binding_deletes {
            if !deletes.insert(binding_id.clone()) {
                return Err(ExtensionStoreError::Conflict(format!(
                    "提交中重复删除绑定 {binding_id}"
                )));
            }
            if upserts.contains(binding_id)
                || baselines.contains(binding_id)
                || baseline_deletes.contains(binding_id)
            {
                return Err(ExtensionStoreError::Conflict(format!(
                    "提交同时写入并删除绑定 {binding_id}"
                )));
            }
        }
        for binding in &commit.pre_bindings {
            super::records::require_current_schema("提交前绑定", binding.schema_version)?;
        }
        for (_, baseline) in &commit.pre_baselines {
            super::records::require_current_schema("提交前基线", baseline.schema_version)?;
        }
        if let Some(snapshot) = &commit.snapshot {
            super::records::validate_stored_snapshot(snapshot)?;
        }
        Ok(())
    }

    /// Restores the library to a journaled operation's pre-state after a
    /// crash between `LibraryPrepared` and `Committed`. Idempotent: replay
    /// writes the same pre-state again.
    pub fn recover_library(&self, commit: &LibraryCommit) -> Result<(), ExtensionStoreError> {
        let _guard = save_lock();
        self.validate_commit(commit)?;
        if self.has_commit_marker(&commit.history_operation_id)? {
            return Err(ExtensionStoreError::Conflict(format!(
                "操作 {} 已完成提交，不能恢复为操作前状态",
                commit.history_operation_id
            )));
        }
        self.restore_library_state_locked(commit)?;
        self.bump_generation_locked()
    }

    /// Restores every binding and baseline touched by `commit` to its
    /// pre-state. Both normal commit-error compensation and crash recovery
    /// use this exact routine, so they cannot drift apart.
    pub(super) fn restore_library_state_locked(
        &self,
        commit: &LibraryCommit,
    ) -> Result<(), ExtensionStoreError> {
        let affected: BTreeSet<String> = commit
            .binding_upserts
            .iter()
            .map(|binding| binding.id.clone())
            .chain(commit.binding_deletes.iter().cloned())
            .chain(commit.baseline_files.iter().map(|(id, _)| id.clone()))
            .chain(commit.baseline_deletes.iter().cloned())
            .collect();
        for binding_id in affected {
            match commit
                .pre_bindings
                .iter()
                .find(|binding| binding.id == binding_id)
            {
                Some(binding) => self.write_binding_locked(binding)?,
                None => self.remove_optional_file(
                    &self.path(&["bindings", &format!("{binding_id}.json")]),
                )?,
            }
            match commit
                .pre_baselines
                .iter()
                .find(|(id, _)| id == &binding_id)
            {
                Some((_, baseline)) => self.write_baseline_locked(&binding_id, baseline)?,
                None => self.remove_optional_file(
                    &self.path(&["baselines", &format!("{binding_id}.json")]),
                )?,
            }
        }
        self.remove_optional_file(
            &self.path(&["history", &format!("{}.json", commit.history_operation_id)]),
        )?;
        self.remove_optional_file(&self.commit_marker_path(&commit.history_operation_id))?;
        Ok(())
    }

    pub(super) fn restore_commit_state_locked(
        &self,
        commit: &LibraryCommit,
        manifest_before: Option<&ExtensionManifest>,
    ) -> Result<(), ExtensionStoreError> {
        let mut failures = Vec::new();
        if let Err(error) = self.restore_library_state_locked(commit) {
            failures.push(error.to_string());
        }
        if let Err(error) = self.restore_manifest_state_locked(manifest_before) {
            failures.push(error.to_string());
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(ExtensionStoreError::Unreadable(failures.join("；")))
        }
    }

    /// Lock-free internals used by [`commit_batch`] and [`recover_library`],
    /// which already hold the library lock.
    pub(super) fn write_binding_locked(
        &self,
        binding: &ExtensionBinding,
    ) -> Result<(), ExtensionStoreError> {
        self.write_json(
            &self.path(&["bindings", &format!("{}.json", binding.id)]),
            binding,
        )
    }

    pub(super) fn write_baseline_locked(
        &self,
        binding_id: &str,
        baseline: &ManagedBaselineFile,
    ) -> Result<(), ExtensionStoreError> {
        self.write_json(
            &self.path(&["baselines", &format!("{binding_id}.json")]),
            baseline,
        )
    }

    pub(super) fn commit_marker_path(&self, operation_id: &str) -> PathBuf {
        self.path(&["commits", &format!("{operation_id}.json")])
    }

    pub(super) fn write_commit_marker_locked(
        &self,
        operation_id: &str,
    ) -> Result<(), ExtensionStoreError> {
        self.write_json(
            &self.commit_marker_path(operation_id),
            &LibraryCommitMarker {
                schema_version: EXTENSIONS_SCHEMA_VERSION,
                operation_id: operation_id.to_string(),
            },
        )
    }

    pub(super) fn remove_optional_file(&self, path: &Path) -> Result<(), ExtensionStoreError> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ExtensionStoreError::Unreadable(error.to_string())),
        }
    }

    pub(super) fn bump_generation_locked(&self) -> Result<(), ExtensionStoreError> {
        let mut manifest = self.manifest()?;
        manifest.generation = manifest.generation.checked_add(1).ok_or_else(|| {
            ExtensionStoreError::Unsupported("扩展库 generation 已达到上限".to_string())
        })?;
        self.write_json(&self.path(&["manifest.json"]), &manifest)
    }
}

pub(super) fn save_lock() -> std::sync::MutexGuard<'static, ()> {
    library_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
