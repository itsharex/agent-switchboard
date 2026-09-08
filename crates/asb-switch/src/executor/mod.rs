//! The switch executor: the only layer allowed to write live configuration
//! files.
//!
//! One switch walks: lock → read & hash → external-change check → preview →
//! render → re-check → backup → temporary write → syntax validation → atomic
//! replacement → post-write verification → lock release. Any failure after
//! the file has been replaced restores the immediately preceding backup.

mod preview;
mod rendered;
mod switch;
mod upgrade;

pub(crate) use preview::read_current_or_empty;
pub use preview::read_preview;
pub use rendered::execute_rendered;
pub(crate) use switch::verify_live_snapshot;
pub use switch::{execute, restore, restore_projected};

use asb_core::{BackupRecord, LockStatus, SwitchPlan, SwitchPreview};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::io::SwitchIo;

pub use crate::restore::RestoreOutcome;
pub const PROCESS_NAME: &str = "agent-switchboard";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub preview: SwitchPreview,
    pub content_hash: String,
    /// Hash of the exact private candidate used for execution. Execution
    /// refuses a plan whose original rendering changed after this preview.
    pub rendered_hash: String,
    /// Redacted candidate file text for the pretty-printed UI view. The
    /// executor keeps the original candidate private for hashing and writes.
    pub content: String,
}

/// What happened to the live file after a failure.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum RecoveryOutcome {
    /// The live file was never replaced; no restore was needed.
    NotNeeded,
    /// The pre-switch backup was restored over a failed write.
    Restored { backup: BackupRecord },
    /// Restoration itself failed; the backup path is reported loudly.
    RestoreFailed { reason: String, backup_path: String },
}

/// Structured execution result. The UI never has to infer any of this.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchOutcome {
    /// Lock acquisition report; the lock is released before returning.
    pub lock: LockStatus,
    pub acquired_at: String,
    pub changed: Vec<String>,
    pub warnings: Vec<String>,
    pub backup: BackupRecord,
    pub preview: SwitchPreview,
    pub recovery: RecoveryOutcome,
    /// SHA-256 hex digest of the file content that is now live; the switch
    /// log records it so later external edits are detectable.
    pub final_hash: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SwitchError {
    /// The target could not be read before planning.
    ReadCurrent { message: String },
    /// Validation or adapter parsing rejected the plan/current content.
    PlanRejected {
        message: String,
        line: Option<usize>,
    },
    /// A lock blocked the switch. Nothing was changed.
    BlockedByLock { status: LockStatus },
    /// The file changed since the preview was produced. Nothing was changed.
    ExternalChange {
        expected_hash: String,
        found_hash: String,
    },
    /// The selected profile or client settings changed after preview. Nothing
    /// was changed because the user has not confirmed the new candidate.
    PlanChanged,
    /// A write-path stage failed; `recovery` says what happened to the file.
    CommitFailed {
        stage: &'static str,
        message: String,
        recovery: RecoveryOutcome,
    },
    /// The transaction reached another result, but its process-owned lock
    /// could not be released. The original result remains available instead
    /// of silently treating a potentially active lock as gone.
    LockReleaseFailed {
        message: String,
        prior: Box<SwitchError>,
    },
}

impl std::fmt::Display for SwitchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwitchError::ReadCurrent { .. } => write!(f, "无法读取当前配置"),
            SwitchError::PlanRejected { message, .. } => write!(f, "配置计划无效：{message}"),
            SwitchError::BlockedByLock { status } => {
                write!(f, "切换被锁阻塞：{}", lock_status_label(status))
            }
            SwitchError::ExternalChange { .. } => {
                write!(f, "配置在预览之后被外部修改，已阻止切换")
            }
            SwitchError::PlanChanged => write!(f, "供应商或客户端设置已变更，请重新查看差异"),
            SwitchError::CommitFailed {
                stage, recovery, ..
            } => {
                write!(
                    f,
                    "写入阶段“{}”失败；恢复结果：{}",
                    stage_label(stage),
                    recovery_label(recovery)
                )
            }
            SwitchError::LockReleaseFailed { prior, .. } => {
                write!(f, "写入锁释放失败；原操作结果：{prior}")
            }
        }
    }
}

impl std::error::Error for SwitchError {}

fn lock_status_label(status: &LockStatus) -> &'static str {
    match status {
        LockStatus::Free => "无锁",
        LockStatus::Held(_) => "有进程正在持锁",
        LockStatus::Stale(_) => "发现遗留锁",
        LockStatus::Indeterminate { .. } => "锁状态无法确定",
    }
}

fn stage_label(stage: &str) -> &'static str {
    match stage {
        "backup-dir" => "准备备份目录",
        "backup" => "创建备份",
        "backup-verify" => "回读校验备份文件",
        "backup-meta" => "记录备份信息",
        "target-dir" => "准备配置目录",
        "temp-write" => "写入临时文件",
        "temp-verify" => "回读校验临时文件",
        "temp-validate" => "校验临时文件",
        "atomic-replace" => "替换配置文件",
        "post-verify" => "验证写入结果",
        "state-save" => "保存应用状态",
        "lock-release" => "释放写入锁",
        "restore-precheck" => "创建恢复前备份",
        "restore-backup-verify" => "校验待恢复备份",
        "restore-precheck-verify" => "回读校验恢复前备份",
        "restore-meta" => "记录恢复前备份信息",
        "restore-write" => "写入恢复临时文件",
        "restore-temp-verify" => "回读校验恢复临时文件",
        "restore-temp-validate" => "校验恢复临时文件",
        "restore-replace" => "替换恢复内容",
        "restore-verify" => "验证恢复结果",
        "transaction-recovery" => "补偿未完成配置事务",
        _ => "提交配置",
    }
}

fn recovery_label(recovery: &RecoveryOutcome) -> &'static str {
    match recovery {
        RecoveryOutcome::NotNeeded => "无需恢复",
        RecoveryOutcome::Restored { .. } => "已恢复上一份备份",
        RecoveryOutcome::RestoreFailed { .. } => "恢复失败，请使用备份文件手动恢复",
    }
}

/// SHA-256 digest bytes for one content string; [`sha256_hex`] is its hex
/// rendering.
pub fn sha256_digest(content: &str) -> Vec<u8> {
    Sha256::digest(content.as_bytes()).to_vec()
}

pub fn sha256_hex(content: &str) -> String {
    sha256_digest(content)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub struct SwitchRequest<'a> {
    pub target: &'a Path,
    pub plan: &'a SwitchPlan,
    pub backup_dir: &'a Path,
    /// Hash captured when the user saw the preview; a mismatch blocks the
    /// switch.
    pub expected_hash: &'a str,
    /// Hash of the candidate file shown in the preview.
    pub expected_rendered_hash: &'a str,
}

/// A narrow, executor-owned write of a pre-rendered current client document.
/// It has the same lock, backup, validation, atomic replacement, and
/// callback rollback contract as a provider projection, without recomputing
/// unrelated provider or client-setting fields.
pub struct RenderedWriteRequest<'a> {
    pub target: &'a Path,
    pub app: asb_core::AppKind,
    pub backup_dir: &'a Path,
    pub expected_hash: &'a str,
    pub expected_target_existed: bool,
    pub rendered: &'a str,
    pub reason: &'a str,
}

#[derive(Debug, Clone)]
pub struct RenderedWriteOutcome {
    pub backup: BackupRecord,
    pub final_hash: String,
    pub warnings: Vec<String>,
}

pub(crate) fn metadata_path(backup_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.meta.json", backup_path.to_string_lossy()))
}

pub(crate) fn write_backup_metadata<Io: SwitchIo>(
    io: &Io,
    record: &BackupRecord,
    stage: &'static str,
) -> Result<(), SwitchError> {
    let meta = serde_json::to_string(record).expect("backup record serializes");
    let backup = Path::new(&record.backup_path);
    let metadata = metadata_path(backup);
    io.write_file_replace(&metadata, &meta)
        .and_then(|_| io.sync_file(backup))
        .and_then(|_| io.sync_file(&metadata))
        .and_then(|_| io.sync_dir(backup.parent().expect("backup directory")))
        .map_err(|error| SwitchError::CommitFailed {
            stage,
            message: error.to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        })
}

static BACKUP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn timestamp_name<Io: SwitchIo>(io: &Io) -> String {
    let timestamp: String = io
        .now_rfc3339()
        .chars()
        .filter(|c| c.is_ascii_digit())
        .take(17)
        .collect();
    let sequence = BACKUP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{timestamp}-{}-{sequence}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_errors_do_not_expose_internal_english_details() {
        let read_error = SwitchError::ReadCurrent {
            message: "access denied".to_string(),
        };
        assert_eq!(format!("{read_error}"), "无法读取当前配置");

        let commit_error = SwitchError::CommitFailed {
            stage: "backup-dir",
            message: "access denied".to_string(),
            recovery: RecoveryOutcome::NotNeeded,
        };
        assert_eq!(
            format!("{commit_error}"),
            "写入阶段“准备备份目录”失败；恢复结果：无需恢复"
        );
    }
}
