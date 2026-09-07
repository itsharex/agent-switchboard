//! Execution of one extension operation as a journaled, reversible batch.
//!
//! Discipline implemented here:
//!
//! 1. The journal is durable before any mutation: every record is a full
//!    line, flushed to disk, and an in-flight document replacement carries
//!    its complete undo information in the `StepStarted` record.
//! 2. Every visible file change enters the rollback set the moment it
//!    happens — including changes that later fail post-write verification.
//!    A failure later in the same target rolls back earlier writes of that
//!    target, not only whole targets.
//! 3. The library commit happens inside the same journal boundary: the
//!    prepared library change is journaled (`LibraryPrepared`) before the
//!    commit closure runs, so crash recovery can restore both the client
//!    files and the library to the pre-operation state.

mod backup;
mod pipeline;
mod prepare;
mod run;
mod undo;
mod verify;

pub use pipeline::apply_extension_plan;

use std::path::Path;

use asb_core::extensions::contracts::{ExtensionOperationRecord, RollbackSummary};
use asb_core::extensions::plan::ExtensionPlan;
use asb_core::extensions::skill::ContentEntry;
use asb_core::extensions::validate::ContentEntryKind;

use crate::io::{PathKind, SwitchIo};

/// The journal sidecar name inside a transaction directory.
pub const JOURNAL_FILE_NAME: &str = "journal.jsonl";

/// Hex SHA-256 of arbitrary bytes (documents and skill files).
pub(crate) fn sha256_bytes_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Everything the executor needs besides the plan itself. The journal
/// directory is owned by this operation; the backup directory comes
/// prepared by the caller (network downloads and staging complete before
/// any lock is taken). `library_commit` is the serialized library change
/// the commit closure will apply; it is journaled so crash recovery can
/// undo a half-applied library batch.
pub struct ExtensionApplyRequest<'a> {
    pub operation_id: &'a str,
    pub plan: &'a ExtensionPlan,
    pub journal_dir: &'a Path,
    pub library_commit: Option<serde_json::Value>,
}

/// Errors from an extension operation. Any variant that carries a record
/// describes a batch that reached the disk and was rolled back (or could
/// not be); the caller persists that record into history either way.
#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    /// The disk state moved on after the plan was prepared. The user must
    /// preview again; the backend never recalculates and writes silently.
    #[error("{message}")]
    PlanStale { message: String },
    /// A required lock is held, stale, or indeterminate.
    #[error("{message}")]
    Blocked { message: String },
    /// The operation failed before any target write happened.
    #[error("{message}")]
    Preparation { message: String },
    /// The operation failed after writing at least one target. The record
    /// carries per-target results; the summary what the rollback achieved.
    #[error("{message}")]
    Failed {
        message: String,
        record: ExtensionOperationRecord,
        rollback: RollbackSummary,
    },
    /// A crash was detected mid-operation and automatic recovery could not
    /// complete; targeted writes stay blocked until the user acts.
    #[error("{message}")]
    RecoveryRequired { message: String },
}

/// Result of the application-owned library commit that follows successful
/// client-file writes. A recoverable rejection lets the executor roll the
/// client files back normally. A failed compensation means the library's
/// journaled pre-state must be recovered before another overlapping write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryCommitError {
    Failed { message: String },
    RecoveryRequired { message: String },
}

impl LibraryCommitError {
    pub fn failed(message: impl Into<String>) -> Self {
        Self::Failed {
            message: message.into(),
        }
    }

    pub fn recovery_required(message: impl Into<String>) -> Self {
        Self::RecoveryRequired {
            message: message.into(),
        }
    }
}

impl From<String> for LibraryCommitError {
    fn from(message: String) -> Self {
        Self::failed(message)
    }
}

impl From<&str> for LibraryCommitError {
    fn from(message: &str) -> Self {
        Self::failed(message)
    }
}

impl ExtensionError {
    fn preparation(message: impl Into<String>) -> Self {
        ExtensionError::Preparation {
            message: message.into(),
        }
    }
}

/// Walks one managed content directory into canon entries (files and
/// directories, links refused). This walker is the single convention shared
/// by import, deploy verification, and baseline digests.
pub fn walk_content_entries<Io: SwitchIo>(
    io: &Io,
    root: &Path,
) -> Result<Vec<ContentEntry>, ExtensionError> {
    let mut entries = Vec::new();
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        let mut children = io.list_dir(&dir).map_err(|error| {
            ExtensionError::preparation(format!("无法读取目录 {}：{error}", dir.display()))
        })?;
        children.sort();
        for child in children {
            let name = child
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            match io.path_kind(&child).map_err(|error| {
                ExtensionError::preparation(format!("无法读取 {}: {error}", child.display()))
            })? {
                PathKind::File { mode, .. } => {
                    let bytes = io.read_bytes(&child).map_err(|error| {
                        ExtensionError::preparation(format!(
                            "无法读取 {}: {error}",
                            child.display()
                        ))
                    })?;
                    entries.push(ContentEntry {
                        relative_path: relative,
                        kind: ContentEntryKind::File,
                        bytes,
                        mode,
                    });
                }
                PathKind::Directory => {
                    entries.push(ContentEntry {
                        relative_path: relative.clone(),
                        kind: ContentEntryKind::Dir,
                        bytes: Vec::new(),
                        mode: 0o755,
                    });
                    stack.push((child, relative));
                }
                PathKind::Absent | PathKind::Other => {
                    return Err(ExtensionError::preparation(format!(
                        "{} 是链接、重解析点或已消失；只接受可完整物化的普通目录",
                        child.display()
                    )));
                }
            }
        }
    }
    Ok(entries)
}

/// The digest recorded for directory entries in managed manifests: an
/// explicit empty string, since directories carry no bytes.
pub fn directory_digest_marker() -> String {
    String::new()
}

/// Copy the tree at `source` into `destination` (which must not exist).
/// Both directories must be regular trees; links abort the copy.
pub(crate) fn copy_tree<Io: SwitchIo>(
    io: &Io,
    source: &Path,
    destination: &Path,
    stage: &'static str,
) -> Result<(), ExtensionError> {
    io.ensure_dir(destination)
        .map_err(|error| ExtensionError::Preparation {
            message: format!(
                "无法创建 {}（阶段 {stage}）：{error}",
                destination.display()
            ),
        })?;
    let entries = walk_content_entries(io, source)?;
    for entry in entries {
        let target_path = destination.join(&entry.relative_path);
        match entry.kind {
            ContentEntryKind::Dir => {
                io.ensure_dir(&target_path)
                    .map_err(|error| ExtensionError::Preparation {
                        message: format!(
                            "无法创建 {}（阶段 {stage}）：{error}",
                            target_path.display()
                        ),
                    })?;
                io.set_mode(&target_path, entry.mode).map_err(|error| {
                    ExtensionError::Preparation {
                        message: format!(
                            "无法设置 {} 的权限（阶段 {stage}）：{error}",
                            target_path.display()
                        ),
                    }
                })?;
            }
            ContentEntryKind::File => {
                io.write_new_bytes(&target_path, &entry.bytes)
                    .map_err(|error| ExtensionError::Preparation {
                        message: format!(
                            "无法写入 {}（阶段 {stage}）：{error}",
                            target_path.display()
                        ),
                    })?;
                io.set_mode(&target_path, entry.mode).map_err(|error| {
                    ExtensionError::Preparation {
                        message: format!(
                            "无法设置 {} 的权限（阶段 {stage}）：{error}",
                            target_path.display()
                        ),
                    }
                })?;
            }
            ContentEntryKind::Link => unreachable!("walker refuses links"),
        }
    }
    Ok(())
}
