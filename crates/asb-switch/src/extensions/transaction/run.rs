//! Step execution: the visible writes themselves, each re-verified after it
//! lands so a failed post-write check still carries its undo information.

use std::path::PathBuf;

use asb_core::extensions::contracts::{AppliedStep, ManagedFileEntry};
use asb_core::extensions::skill::content_digest;

use super::prepare::{PreparedStep, StepError, StepFailure};
use super::verify::document_hash;
use super::{copy_tree, walk_content_entries};
use crate::executor::timestamp_name;
use crate::io::{PathKind, SwitchIo};

pub(super) fn run_prepared_step<Io: SwitchIo>(
    io: &Io,
    prepared: PreparedStep,
) -> Result<AppliedStep, StepFailure> {
    match prepared {
        PreparedStep::Document {
            undo,
            expected_final_hash,
            temp_path,
            path,
            expected_hash,
            ..
        } => {
            // Final conflict re-check right before the irreversible replace.
            let recheck =
                document_hash(io, &path).map_err(|error| StepFailure::bare(error.to_string()))?;
            if recheck != expected_hash {
                let _ = io.remove(&temp_path);
                return Err(StepFailure::bare(format!(
                    "{} 已在预览后变化；请重新预览",
                    path.display()
                )));
            }
            io.rename_replace(&temp_path, &path)
                .map_err(|error| StepFailure::bare(format!("原子替换失败：{error}")))?;
            // Post-write verification. A failure here still leaves our
            // write on disk: the undo record travels with the failure so
            // the batch rollback reverts it.
            match document_hash(io, &path) {
                Ok(Some(actual)) if actual == expected_final_hash => Ok(undo),
                other => {
                    let observed = match other {
                        Ok(Some(actual)) => Some(actual),
                        _ => None,
                    };
                    Err(StepFailure {
                        error: StepError::from(
                            "写后验证失败：目标内容与渲染结果不一致".to_string(),
                        ),
                        completed: Some(with_observed_hash(undo, observed)),
                    })
                }
            }
        }
        PreparedStep::DocumentRemove {
            undo,
            path,
            expected_hash,
        } => {
            let recheck =
                document_hash(io, &path).map_err(|error| StepFailure::bare(error.to_string()))?;
            if recheck.as_deref() != Some(expected_hash.as_str()) {
                return Err(StepFailure::bare(format!(
                    "{} 已在预览后变化；请重新预览",
                    path.display()
                )));
            }
            io.remove(&path).map_err(|error| {
                StepFailure::bare(format!("移除 {} 失败：{error}", path.display()))
            })?;
            match document_hash(io, &path) {
                Ok(None) => Ok(undo),
                _ => Err(StepFailure {
                    error: StepError::from("文档移除后仍存在；请检查占用".to_string()),
                    completed: Some(undo),
                }),
            }
        }
        PreparedStep::Deploy {
            target_dir,
            staging_dir,
            files,
            original_digest,
            backup_dir,
            staged_digest,
        } => run_directory_deploy(
            io,
            target_dir,
            staging_dir,
            &files,
            original_digest,
            backup_dir,
            staged_digest,
        ),
        PreparedStep::Remove {
            target_dir,
            digest,
            backup_path,
            ..
        } => {
            io.remove_dir_all(&target_dir)
                .map_err(|error| StepFailure::bare(format!("目录移除失败：{error}")))?;
            let still_present = match io.path_kind(&target_dir) {
                Ok(PathKind::Absent) => false,
                Ok(_) => true,
                Err(error) => return Err(StepFailure::bare(error.to_string())),
            };
            if still_present {
                return Err(StepFailure {
                    error: StepError::from("目录移除后仍存在；请检查占用".to_string()),
                    completed: Some(AppliedStep::DirectoryRemoved {
                        target_dir: target_dir.to_string_lossy().to_string(),
                        backup_reference: backup_path.to_string_lossy().to_string(),
                        digest,
                        original_existed: true,
                    }),
                });
            }
            Ok(AppliedStep::DirectoryRemoved {
                target_dir: target_dir.to_string_lossy().to_string(),
                backup_reference: backup_path.to_string_lossy().to_string(),
                digest,
                original_existed: true,
            })
        }
    }
}

/// The undo record for a failed post-verification verifies against the
/// state we last observed, not the state we intended: undo refuses to
/// touch a target that moved on again after our write.
fn with_observed_hash(undo: AppliedStep, observed: Option<String>) -> AppliedStep {
    match (undo, observed) {
        (
            AppliedStep::DocumentWritten {
                path,
                syntax,
                backup_reference,
                written_hash: _,
                original_hash,
            },
            Some(actual),
        ) => AppliedStep::DocumentWritten {
            path,
            syntax,
            backup_reference,
            written_hash: actual,
            original_hash,
        },
        (other, _) => other,
    }
}

#[allow(clippy::too_many_arguments)]
fn run_directory_deploy<Io: SwitchIo>(
    io: &Io,
    target_dir: PathBuf,
    staging_dir: PathBuf,
    files: &[ManagedFileEntry],
    original_digest: Option<String>,
    backup_dir: PathBuf,
    staged_digest: String,
) -> Result<AppliedStep, StepFailure> {
    let original_existed = original_digest.is_some();
    // 1. Copy the new content to a same-volume sibling and verify it there.
    let file_name = target_dir
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "skill".to_string());
    let new_dir =
        target_dir.with_file_name(format!("{file_name}.{}.asb-ext-new", std::process::id()));
    let _ = io.remove_dir_all(&new_dir);
    copy_tree(io, &staging_dir, &new_dir, "stage-copy")
        .map_err(|error| StepFailure::bare(error.to_string()))?;
    let copied =
        walk_content_entries(io, &new_dir).map_err(|error| StepFailure::bare(error.to_string()))?;
    if content_digest(&copied) != staged_digest {
        let _ = io.remove_dir_all(&new_dir);
        return Err(StepFailure::bare(
            "部署副本校验失败：复制后的内容与暂存不一致",
        ));
    }
    let _ = files;
    // 2. Back up the current directory (copy, volume-agnostic).
    let mut backup_reference = None;
    let mut old_dir: Option<PathBuf> = None;
    if original_existed {
        io.ensure_dir(&backup_dir)
            .map_err(|error| StepFailure::bare(format!("无法创建备份目录：{error}")))?;
        let backup_path = backup_dir.join(format!("{file_name}.{}.bak", timestamp_name(io)));
        let _ = io.remove_dir_all(&backup_path);
        copy_tree(io, &target_dir, &backup_path, "dir-backup")
            .map_err(|error| StepFailure::bare(error.to_string()))?;
        let backup_entries = walk_content_entries(io, &backup_path)
            .map_err(|error| StepFailure::bare(error.to_string()))?;
        if content_digest(&backup_entries) != original_digest.as_deref().unwrap_or_default() {
            let _ = io.remove_dir_all(&backup_path);
            let _ = io.remove_dir_all(&new_dir);
            return Err(StepFailure::bare("目录备份校验失败：备份内容与目标不一致"));
        }
        backup_reference = Some(backup_path.to_string_lossy().to_string());
    }
    // 3. Swap: move the old tree aside, move the new tree in.
    if original_existed {
        let old_path =
            target_dir.with_file_name(format!("{file_name}.{}.asb-ext-old", std::process::id()));
        io.rename(&target_dir, &old_path).map_err(|error| {
            let _ = io.remove_dir_all(&new_dir);
            StepFailure::bare(format!(
                "旧目录移出失败（可能被占用）：{error}；请关闭正在使用该目录的会话后重试"
            ))
        })?;
        old_dir = Some(old_path);
    }
    if let Err(error) = io.rename(&new_dir, &target_dir) {
        // Undo the half swap.
        if let Some(old_path) = &old_dir {
            let _ = io.rename(old_path, &target_dir);
        }
        let _ = io.remove_dir_all(&new_dir);
        return Err(StepFailure::bare(format!("新目录移入失败：{error}")));
    }
    // 4. Post-write verification, then drop the moved-out old tree.
    let deployed = walk_content_entries(io, &target_dir)
        .map_err(|error| StepFailure::bare(error.to_string()))?;
    if content_digest(&deployed) != staged_digest {
        return Err(StepFailure {
            error: StepError::from("写后验证失败：部署后的目录与计划不一致".to_string()),
            completed: Some(AppliedStep::DirectoryDeployed {
                target_dir: target_dir.to_string_lossy().to_string(),
                backup_reference: backup_reference.clone(),
                digest: content_digest(&deployed),
                original_digest,
            }),
        });
    }
    if let Some(old_path) = &old_dir {
        let _ = io.remove_dir_all(old_path);
    }
    Ok(AppliedStep::DirectoryDeployed {
        target_dir: target_dir.to_string_lossy().to_string(),
        backup_reference,
        digest: staged_digest,
        original_digest,
    })
}
