//! Step preparation: every check, backup, and staging side effect that must
//! precede a visible change, plus the undo record journaled before the run.

use std::path::{Path, PathBuf};

use asb_core::extensions::contracts::{AppliedStep, DocumentSyntax, ManagedFileEntry};
use asb_core::extensions::plan::PlanStep;
use asb_core::extensions::skill::content_digest;

use super::backup::backup_document;
use super::verify::{document_hash, validate_rendered_syntax, verify_managed_files};
use super::{copy_tree, sha256_bytes_hex, walk_content_entries};
use crate::executor::timestamp_name;
use crate::io::{PathKind, SwitchIo};

// ---------------------------------------------------------------- step execution

/// A step failure that is either a stale plan (nothing written; the user
/// re-previews) or anything else, optionally carrying the undo information
/// of a write that already reached the disk.
pub(super) struct StepFailure {
    pub(super) error: StepError,
    pub(super) completed: Option<AppliedStep>,
}

impl StepFailure {
    pub(super) fn bare(message: impl Into<String>) -> Self {
        Self {
            error: StepError::from(message.into()),
            completed: None,
        }
    }

    fn stale_check(message: &str) -> bool {
        message.contains("请重新预览")
    }
}

pub(super) enum StepError {
    Stale(String),
    Other(String),
}

impl From<String> for StepError {
    fn from(message: String) -> Self {
        // Every stale rejection in this module names the re-preview action;
        // infrastructure and verification failures do not.
        if StepFailure::stale_check(&message) {
            StepError::Stale(message)
        } else {
            StepError::Other(message)
        }
    }
}

impl StepError {
    pub(super) fn message(&self) -> &str {
        match self {
            StepError::Stale(message) | StepError::Other(message) => message,
        }
    }

    pub(super) fn is_stale(&self) -> bool {
        matches!(self, StepError::Stale(_))
    }
}

/// A fully prepared step. The preparation phase performs every check and
/// side effect that must precede the visible change (backup, staging,
/// syntax validation); `undo` describes how to revert the change the run
/// phase is about to make, and is journaled before the run starts.
pub(super) enum PreparedStep {
    Document {
        undo: AppliedStep,
        expected_final_hash: String,
        temp_path: PathBuf,
        path: PathBuf,
        expected_hash: Option<String>,
    },
    DocumentRemove {
        undo: AppliedStep,
        path: PathBuf,
        expected_hash: String,
    },
    Deploy {
        target_dir: PathBuf,
        staging_dir: PathBuf,
        files: Vec<ManagedFileEntry>,
        original_digest: Option<String>,
        backup_dir: PathBuf,
        staged_digest: String,
    },
    Remove {
        target_dir: PathBuf,
        digest: String,
        backup_path: PathBuf,
    },
}

impl PreparedStep {
    pub(super) fn undo(&self) -> Option<AppliedStep> {
        match self {
            // Document writes carry complete undo info before the replace:
            // the crash window between replacement and the completion
            // record stays recoverable.
            PreparedStep::Document { undo, .. } => Some(undo.clone()),
            PreparedStep::DocumentRemove { undo, .. } => Some(undo.clone()),
            // Directory removals: the backup exists and the target is still
            // intact when the record is written; removal is the final act.
            PreparedStep::Remove {
                target_dir,
                digest,
                backup_path,
                ..
            } => Some(AppliedStep::DirectoryRemoved {
                target_dir: target_dir.to_string_lossy().to_string(),
                backup_reference: backup_path.to_string_lossy().to_string(),
                digest: digest.clone(),
                original_existed: true,
            }),
            // Directory deploys swap in place; an in-flight swap is swept
            // from the sibling artifacts instead of a pre-recorded undo.
            PreparedStep::Deploy { .. } => None,
        }
    }

    pub(super) fn describe(&self) -> (crate::extensions::recovery::StepKind, String) {
        use crate::extensions::recovery::StepKind;
        match self {
            PreparedStep::Document { path, .. } => {
                (StepKind::Document, path.to_string_lossy().to_string())
            }
            PreparedStep::DocumentRemove { path, .. } => {
                (StepKind::Document, path.to_string_lossy().to_string())
            }
            PreparedStep::Deploy { target_dir, .. } => {
                (StepKind::Deploy, target_dir.to_string_lossy().to_string())
            }
            PreparedStep::Remove { target_dir, .. } => {
                (StepKind::Remove, target_dir.to_string_lossy().to_string())
            }
        }
    }
}

pub(super) fn prepare_step<Io: SwitchIo>(
    io: &Io,
    step: &PlanStep,
) -> Result<PreparedStep, StepFailure> {
    match step {
        PlanStep::DocumentWrite {
            path,
            expected_content_hash,
            rendered,
            syntax,
            backup_dir,
            ..
        } => prepare_document_write(
            io,
            Path::new(path),
            expected_content_hash.clone(),
            rendered,
            *syntax,
            Path::new(backup_dir),
        ),
        PlanStep::DocumentRemove {
            path,
            expected_content_hash,
            syntax,
            backup_dir,
            ..
        } => prepare_document_remove(
            io,
            Path::new(path),
            expected_content_hash,
            *syntax,
            Path::new(backup_dir),
        ),
        PlanStep::DirectoryDeploy {
            target_dir,
            staging_dir,
            files,
            original_digest,
            backup_dir,
            ..
        } => prepare_directory_deploy(
            io,
            Path::new(target_dir),
            Path::new(staging_dir),
            files,
            original_digest.clone(),
            Path::new(backup_dir),
        ),
        PlanStep::DirectoryRemove {
            target_dir,
            expected_files,
            expected_digest,
            backup_dir,
            ..
        } => prepare_directory_remove(
            io,
            Path::new(target_dir),
            expected_files,
            expected_digest.clone(),
            Path::new(backup_dir),
        ),
    }
}

fn prepare_document_write<Io: SwitchIo>(
    io: &Io,
    path: &Path,
    expected_content_hash: Option<String>,
    rendered: &str,
    syntax: DocumentSyntax,
    backup_dir: &Path,
) -> Result<PreparedStep, StepFailure> {
    let path = path.to_path_buf();
    // 1. Verify the document still matches preparation.
    let current_hash =
        document_hash(io, &path).map_err(|error| StepFailure::bare(error.to_string()))?;
    if current_hash != expected_content_hash {
        return Err(StepFailure::bare(format!(
            "{} 已在预览后发生变化；请重新预览",
            path.display()
        )));
    }
    // 2. Syntax-validate the candidate before anything is staged.
    validate_rendered_syntax(syntax, rendered).map_err(StepFailure::bare)?;
    // 3. Back up the current content; the backup is part of preparation.
    let mut backup_reference = None;
    if current_hash.is_some() {
        let bytes = io
            .read_bytes(&path)
            .map_err(|error| StepFailure::bare(format!("无法读取 {}：{error}", path.display())))?;
        let backup_path = backup_document(io, &path, &bytes, backup_dir, "ext-backup")
            .map_err(|error| StepFailure::bare(error.to_string()))?;
        backup_reference = Some(backup_path.to_string_lossy().to_string());
    }
    // 4. Stage the candidate and read it back.
    let temp_path = path.with_file_name(format!(
        "{}.{}.asb-ext-tmp",
        path.file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default(),
        std::process::id()
    ));
    let _ = io.remove(&temp_path);
    io.write_new_bytes(&temp_path, rendered.as_bytes())
        .map_err(|error| StepFailure::bare(format!("无法写入暂存文件：{error}")))?;
    let staged = io
        .read_bytes(&temp_path)
        .map_err(|error| StepFailure::bare(format!("暂存文件回读失败：{error}")))?;
    if staged != rendered.as_bytes() {
        let _ = io.remove(&temp_path);
        return Err(StepFailure::bare("暂存文件回读不一致"));
    }
    let written_hash = sha256_bytes_hex(rendered.as_bytes());
    let expected_final_hash = written_hash.clone();
    Ok(PreparedStep::Document {
        undo: AppliedStep::DocumentWritten {
            path: path.to_string_lossy().to_string(),
            syntax,
            backup_reference,
            written_hash,
            original_hash: expected_content_hash.clone(),
        },
        expected_final_hash,
        temp_path,
        path,
        expected_hash: expected_content_hash,
    })
}

fn prepare_document_remove<Io: SwitchIo>(
    io: &Io,
    path: &Path,
    expected_content_hash: &str,
    syntax: DocumentSyntax,
    backup_dir: &Path,
) -> Result<PreparedStep, StepFailure> {
    let path = path.to_path_buf();
    let current_hash =
        document_hash(io, &path).map_err(|error| StepFailure::bare(error.to_string()))?;
    if current_hash.as_deref() != Some(expected_content_hash) {
        return Err(StepFailure::bare(format!(
            "{} 已在预览后发生变化；请重新预览",
            path.display()
        )));
    }
    let bytes = io
        .read_bytes(&path)
        .map_err(|error| StepFailure::bare(format!("无法读取 {}：{error}", path.display())))?;
    let backup_path = backup_document(io, &path, &bytes, backup_dir, "ext-backup")
        .map_err(|error| StepFailure::bare(error.to_string()))?;
    Ok(PreparedStep::DocumentRemove {
        undo: AppliedStep::DocumentRemoved {
            path: path.to_string_lossy().to_string(),
            syntax,
            backup_reference: backup_path.to_string_lossy().to_string(),
            original_hash: expected_content_hash.to_string(),
        },
        path,
        expected_hash: expected_content_hash.to_string(),
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_directory_deploy<Io: SwitchIo>(
    io: &Io,
    target_dir: &Path,
    staging_dir: &Path,
    files: &[asb_core::extensions::plan::PlannedFile],
    original_digest: Option<String>,
    backup_dir: &Path,
) -> Result<PreparedStep, StepFailure> {
    let managed_files = planned_to_managed(files);
    let target_dir = target_dir.to_path_buf();
    // 1. Verify the staging copy matches the plan.
    let staged_entries = walk_content_entries(io, staging_dir)
        .map_err(|error| StepFailure::bare(error.to_string()))?;
    let staged_digest = verify_managed_files(&staged_entries, &managed_files, "暂存副本")
        .map_err(|error| StepFailure::bare(error.to_string()))?;
    // 2. Verify the current target still matches preparation.
    let target_kind = io.path_kind(&target_dir).map_err(|error| {
        StepFailure::bare(format!("无法读取 {}：{error}", target_dir.display()))
    })?;
    let original_existed = original_digest.is_some();
    match (&target_kind, original_existed) {
        (PathKind::Absent, false) => {}
        (PathKind::Directory, true) => {
            let current = walk_content_entries(io, &target_dir)
                .map_err(|error| StepFailure::bare(error.to_string()))?;
            let digest = content_digest(&current);
            if Some(&digest) != original_digest.as_ref() {
                return Err(StepFailure::bare(format!(
                    "{} 已在预览后发生变化；请重新预览",
                    target_dir.display()
                )));
            }
        }
        _ => {
            return Err(StepFailure::bare(format!(
                "{} 的当前状态与准备计划时不一致；请重新预览",
                target_dir.display()
            )));
        }
    }
    Ok(PreparedStep::Deploy {
        target_dir,
        staging_dir: staging_dir.to_path_buf(),
        files: managed_files,
        original_digest,
        backup_dir: backup_dir.to_path_buf(),
        staged_digest,
    })
}

fn prepare_directory_remove<Io: SwitchIo>(
    io: &Io,
    target_dir: &Path,
    expected_files: &[asb_core::extensions::plan::PlannedFile],
    expected_digest: String,
    backup_dir: &Path,
) -> Result<PreparedStep, StepFailure> {
    let managed_files = planned_to_managed(expected_files);
    let target_dir = target_dir.to_path_buf();
    let entries = walk_content_entries(io, &target_dir).map_err(|error| {
        StepFailure::bare(format!("{} 无法读取：{error}", target_dir.display()))
    })?;
    let digest = verify_managed_files(&entries, &managed_files, "待移除目录")
        .map_err(|error| StepFailure::bare(error.to_string()))?;
    if digest != expected_digest {
        return Err(StepFailure::bare(format!(
            "{} 已在预览后发生变化；请重新预览",
            target_dir.display()
        )));
    }
    let file_name = target_dir
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "skill".to_string());
    io.ensure_dir(backup_dir)
        .map_err(|error| StepFailure::bare(format!("无法创建备份目录：{error}")))?;
    let backup_path = backup_dir.join(format!("{file_name}.{}.bak", timestamp_name(io)));
    let _ = io.remove_dir_all(&backup_path);
    copy_tree(io, &target_dir, &backup_path, "dir-backup")
        .map_err(|error| StepFailure::bare(error.to_string()))?;
    let backup_entries = walk_content_entries(io, &backup_path)
        .map_err(|error| StepFailure::bare(error.to_string()))?;
    if content_digest(&backup_entries) != expected_digest {
        let _ = io.remove_dir_all(&backup_path);
        return Err(StepFailure::bare("目录备份校验失败：备份内容与目标不一致"));
    }
    Ok(PreparedStep::Remove {
        target_dir,
        digest: expected_digest,
        backup_path,
    })
}

fn planned_to_managed(files: &[asb_core::extensions::plan::PlannedFile]) -> Vec<ManagedFileEntry> {
    files
        .iter()
        .map(|file| ManagedFileEntry {
            relative_path: file.relative_path.clone(),
            digest: file.digest.clone(),
            mode: file.mode,
            size: file.size,
        })
        .collect()
}
