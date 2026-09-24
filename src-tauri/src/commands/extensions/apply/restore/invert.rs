//! Inverting completed steps: the inverse of each recorded filesystem
//! step, the currency checks that gate a historical restore, and the
//! file/baseline projections a restore preview needs.

//! Historical restore: replaying a finished operation's completed steps
//! backwards, verified against the current client and library state.

use std::fs;

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{
    ExtensionOperationRecord, ManagedBaselineFile, OperationSnapshot,
};
use asb_core::extensions::plan::{ExtensionPlan, PlanStep, PlannedFile, PlannedTarget};
use asb_core::extensions::skill::ContentEntry;

use crate::commands::error::CommandError;
use crate::commands::extensions::support::*;
use crate::extensions::store::ExtensionStore;

pub(super) fn ensure_restore_is_current(
    store: &ExtensionStore,
    snapshot: &OperationSnapshot,
) -> Result<(), CommandError> {
    let bindings = store.list_bindings().map_err(store_error)?;
    let baselines = current_baseline_files(store)?;
    for post in &snapshot.post_bindings {
        if !bindings.iter().any(|current| current == post) {
            return Err(CommandError::keyed(
                "plan-stale",
                "errors.extops.libraryChangedSinceOperation",
                "扩展库已在该操作后发生变化；请恢复最新操作",
            ));
        }
    }
    for pre in &snapshot.pre_bindings {
        if !snapshot.post_bindings.iter().any(|post| post.id == pre.id)
            && bindings.iter().any(|current| current.id == pre.id)
        {
            return Err(CommandError::keyed(
                "plan-stale",
                "errors.extops.libraryChangedSinceOperation",
                "扩展库已在该操作后发生变化；请恢复最新操作",
            ));
        }
    }
    for (id, post) in &snapshot.post_baselines {
        if !baselines
            .iter()
            .any(|(current_id, current)| current_id == id && current == post)
        {
            return Err(CommandError::keyed(
                "plan-stale",
                "errors.extops.baselineChangedSinceOperation",
                "扩展基线已在该操作后发生变化；请恢复最新操作",
            ));
        }
    }
    for (id, _) in &snapshot.pre_baselines {
        if !snapshot
            .post_baselines
            .iter()
            .any(|(post_id, _)| post_id == id)
            && baselines.iter().any(|(current_id, _)| current_id == id)
        {
            return Err(CommandError::keyed(
                "plan-stale",
                "errors.extops.baselineChangedSinceOperation",
                "扩展基线已在该操作后发生变化；请恢复最新操作",
            ));
        }
    }
    let mut checked_paths = std::collections::BTreeSet::new();
    for completed in snapshot.completed_steps.iter().rev() {
        let key = completed_step_path(&completed.step);
        if checked_paths.insert(key) {
            verify_completed_step_current(&completed.step)?;
        }
    }
    Ok(())
}

pub(super) fn append_restore_targets(
    plan: &mut ExtensionPlan,
    snapshot: &OperationSnapshot,
    record: &ExtensionOperationRecord,
    backup_root: &std::path::Path,
) -> Result<(), CommandError> {
    for completed in snapshot.completed_steps.iter().rev() {
        let inverse =
            inverse_completed_step(&completed.step, completed.target.client(), backup_root)?;
        let target = &completed.target;
        // The inverse lands under the resource that originally owned the
        // step; two resources may share one target (a skill directory plus
        // an MCP entry in the same client scope), so the stored resource id
        // decides, never the target alone.
        let resource_index = record
            .resources
            .iter()
            .position(|resource| resource.definition_id == completed.resource_id)
            .unwrap_or(0);
        let operation = &mut plan.operations[resource_index];
        if let Some(last) = operation
            .targets
            .last_mut()
            .filter(|last| last.target == *target)
        {
            last.steps.push(inverse);
            continue;
        }
        let binding = snapshot
            .pre_bindings
            .iter()
            .find(|binding| binding.target == *target)
            .or_else(|| {
                snapshot
                    .post_bindings
                    .iter()
                    .find(|binding| binding.target == *target)
            })
            .cloned()
            .unwrap_or_else(|| restore_binding_stub(&operation.definition_id, target));
        let baseline = snapshot
            .pre_baselines
            .iter()
            .find(|(id, _)| id == &binding.id)
            .map(|(_, baseline)| baseline.clone());
        operation.targets.push(PlannedTarget {
            binding_id: Some(binding.id.clone()),
            target: target.clone(),
            binding,
            baseline,
            steps: vec![inverse],
            warnings: vec!["将恢复该操作实际修改的客户端状态".to_string()],
            changes: Vec::new(),
            adopts_native_entry: false,
        });
    }
    Ok(())
}

pub(super) fn inverse_completed_step(
    step: &asb_core::extensions::contracts::AppliedStep,
    client: AppKind,
    backup_root: &std::path::Path,
) -> Result<PlanStep, CommandError> {
    use asb_core::extensions::contracts::AppliedStep;

    match step {
        AppliedStep::DocumentWritten {
            path,
            syntax,
            backup_reference,
            written_hash,
            original_hash,
        } => match original_hash {
            Some(original_hash) => {
                let backup = backup_reference.as_ref().ok_or_else(|| {
                    CommandError::keyed(
                        "extension-invalid",
                        "errors.extops.historyDocBackupRefMissing",
                        "历史文档备份引用缺失",
                    )
                })?;
                let bytes = fs::read(backup).map_err(|error| {
                    CommandError::localized(
                        "extension-invalid",
                        "errors.extops.historyDocBackupUnreadable",
                        format!("历史文档备份无法读取：{error}"),
                        serde_json::json!({ "detail": error.to_string() }),
                    )
                })?;
                if sha_hex(&bytes) != *original_hash {
                    return Err(CommandError::keyed(
                        "extension-invalid",
                        "errors.extops.historyDocBackupDigestMismatch",
                        "历史文档备份摘要不匹配",
                    ));
                }
                let rendered = String::from_utf8(bytes).map_err(|_| {
                    CommandError::keyed(
                        "extension-invalid",
                        "errors.extops.historyDocBackupNotUtf8",
                        "历史文档备份不是 UTF-8 文本",
                    )
                })?;
                Ok(PlanStep::DocumentWrite {
                    client,
                    path: path.clone(),
                    expected_content_hash: Some(written_hash.clone()),
                    expected_existed: true,
                    rendered,
                    syntax: *syntax,
                    backup_dir: backup_root.to_string_lossy().to_string(),
                })
            }
            None => Ok(PlanStep::DocumentRemove {
                client,
                path: path.clone(),
                expected_content_hash: written_hash.clone(),
                syntax: *syntax,
                backup_dir: backup_root.to_string_lossy().to_string(),
            }),
        },
        AppliedStep::DocumentRemoved {
            path,
            syntax,
            backup_reference,
            original_hash,
        } => {
            let bytes = fs::read(backup_reference).map_err(|error| {
                CommandError::localized(
                    "extension-invalid",
                    "errors.extops.historyDocBackupUnreadable",
                    format!("历史文档备份无法读取：{error}"),
                    serde_json::json!({ "detail": error.to_string() }),
                )
            })?;
            if sha_hex(&bytes) != *original_hash {
                return Err(CommandError::keyed(
                    "extension-invalid",
                    "errors.extops.historyDocBackupDigestMismatch",
                    "历史文档备份摘要不匹配",
                ));
            }
            let rendered = String::from_utf8(bytes).map_err(|_| {
                CommandError::keyed(
                    "extension-invalid",
                    "errors.extops.historyDocBackupNotUtf8",
                    "历史文档备份不是 UTF-8 文本",
                )
            })?;
            Ok(PlanStep::DocumentWrite {
                client,
                path: path.clone(),
                expected_content_hash: None,
                expected_existed: false,
                rendered,
                syntax: *syntax,
                backup_dir: backup_root.to_string_lossy().to_string(),
            })
        }
        AppliedStep::DirectoryDeployed {
            target_dir,
            backup_reference,
            digest,
            original_digest,
        } => match original_digest {
            Some(original_digest) => {
                let backup = backup_reference.as_ref().ok_or_else(|| {
                    CommandError::keyed(
                        "extension-invalid",
                        "errors.extops.historyDirBackupRefMissing",
                        "历史目录备份引用缺失",
                    )
                })?;
                let entries = asb_switch::extensions::walk_content_entries(
                    &asb_switch::FsIo,
                    std::path::Path::new(backup),
                )
                .map_err(|error| CommandError::new("extension-invalid", error.to_string()))?;
                if asb_core::extensions::skill::content_digest(&entries) != *original_digest {
                    return Err(CommandError::keyed(
                        "extension-invalid",
                        "errors.extops.historyDirBackupDigestMismatch",
                        "历史目录备份摘要不匹配",
                    ));
                }
                Ok(PlanStep::DirectoryDeploy {
                    client,
                    target_dir: target_dir.clone(),
                    staging_dir: backup.clone(),
                    files: planned_files_from(&entries),
                    original_digest: Some(digest.clone()),
                    backup_dir: backup_root.to_string_lossy().to_string(),
                })
            }
            None => {
                let entries = asb_switch::extensions::walk_content_entries(
                    &asb_switch::FsIo,
                    std::path::Path::new(target_dir),
                )
                .map_err(|error| CommandError::new("plan-stale", error.to_string()))?;
                Ok(PlanStep::DirectoryRemove {
                    client,
                    target_dir: target_dir.clone(),
                    expected_files: planned_files_from(&entries),
                    expected_digest: digest.clone(),
                    backup_dir: backup_root.to_string_lossy().to_string(),
                })
            }
        },
        AppliedStep::DirectoryRemoved {
            target_dir,
            backup_reference,
            digest,
            ..
        } => {
            let entries = asb_switch::extensions::walk_content_entries(
                &asb_switch::FsIo,
                std::path::Path::new(backup_reference),
            )
            .map_err(|error| CommandError::new("extension-invalid", error.to_string()))?;
            if asb_core::extensions::skill::content_digest(&entries) != *digest {
                return Err(CommandError::keyed(
                    "extension-invalid",
                    "errors.extops.historyDirBackupDigestMismatch",
                    "历史目录备份摘要不匹配",
                ));
            }
            Ok(PlanStep::DirectoryDeploy {
                client,
                target_dir: target_dir.clone(),
                staging_dir: backup_reference.clone(),
                files: planned_files_from(&entries),
                original_digest: None,
                backup_dir: backup_root.to_string_lossy().to_string(),
            })
        }
    }
}

pub(super) fn completed_step_path(step: &asb_core::extensions::contracts::AppliedStep) -> String {
    use asb_core::extensions::contracts::AppliedStep;

    match step {
        AppliedStep::DocumentWritten { path, .. } | AppliedStep::DocumentRemoved { path, .. } => {
            format!("document:{path}")
        }
        AppliedStep::DirectoryDeployed { target_dir, .. }
        | AppliedStep::DirectoryRemoved { target_dir, .. } => format!("directory:{target_dir}"),
    }
}

pub(super) fn verify_completed_step_current(
    step: &asb_core::extensions::contracts::AppliedStep,
) -> Result<(), CommandError> {
    use asb_core::extensions::contracts::AppliedStep;

    match step {
        AppliedStep::DocumentWritten {
            path, written_hash, ..
        } => {
            if current_document_hash(std::path::Path::new(path))?.as_deref()
                != Some(written_hash.as_str())
            {
                return Err(CommandError::keyed(
                    "plan-stale",
                    "errors.extops.clientDocChangedSinceOperation",
                    "客户端文档已在该操作后发生变化；请恢复最新操作",
                ));
            }
        }
        AppliedStep::DocumentRemoved { path, .. } => {
            if current_document_hash(std::path::Path::new(path))?.is_some() {
                return Err(CommandError::keyed(
                    "plan-stale",
                    "errors.extops.clientDocChangedSinceOperation",
                    "客户端文档已在该操作后发生变化；请恢复最新操作",
                ));
            }
        }
        AppliedStep::DirectoryDeployed {
            target_dir, digest, ..
        } => {
            let entries = asb_switch::extensions::walk_content_entries(
                &asb_switch::FsIo,
                std::path::Path::new(target_dir),
            )
            .map_err(|_| {
                CommandError::keyed(
                    "plan-stale",
                    "errors.extops.clientDirChangedSinceOperation",
                    "客户端目录已在该操作后发生变化；请恢复最新操作",
                )
            })?;
            if asb_core::extensions::skill::content_digest(&entries) != *digest {
                return Err(CommandError::keyed(
                    "plan-stale",
                    "errors.extops.clientDirChangedSinceOperation",
                    "客户端目录已在该操作后发生变化；请恢复最新操作",
                ));
            }
        }
        AppliedStep::DirectoryRemoved { target_dir, .. } => {
            if std::path::Path::new(target_dir).exists() {
                return Err(CommandError::keyed(
                    "plan-stale",
                    "errors.extops.clientDirChangedSinceOperation",
                    "客户端目录已在该操作后发生变化；请恢复最新操作",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn current_document_hash(
    path: &std::path::Path,
) -> Result<Option<String>, CommandError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => fs::read(path)
            .map(|bytes| Some(sha_hex(&bytes)))
            .map_err(|error| CommandError::new("extension-invalid", error.to_string())),
        Ok(_) => Err(CommandError::keyed(
            "extension-invalid",
            "errors.extops.historyDocPathNotFile",
            "历史文档路径不再是普通文件",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CommandError::new("extension-invalid", error.to_string())),
    }
}

pub(super) fn planned_files_from(entries: &[ContentEntry]) -> Vec<PlannedFile> {
    entries
        .iter()
        .map(|entry| PlannedFile {
            relative_path: entry.relative_path.clone(),
            digest: if entry.kind == asb_core::extensions::validate::ContentEntryKind::Dir {
                String::new()
            } else {
                sha_hex(&entry.bytes)
            },
            mode: entry.mode,
            size: entry.bytes.len() as u64,
        })
        .collect()
}

/// The current baseline files of every stored binding, for a restore's own
/// pre-state snapshot.
pub(super) fn current_baseline_files(
    store: &ExtensionStore,
) -> Result<Vec<(String, ManagedBaselineFile)>, CommandError> {
    let mut files = Vec::new();
    for binding in store.list_bindings().map_err(store_error)? {
        if let Some(baseline) = store.get_baseline_file(&binding.id).map_err(store_error)? {
            files.push((binding.id, baseline));
        }
    }
    Ok(files)
}
