//! Rollback of completed steps and the durable markers of a rolled-back batch.

use asb_core::extensions::contracts::{
    AppliedExtensionStep, ExtensionOperationRecord, RollbackSummary, TargetOutcome,
};

use super::pipeline::append_journal;
use super::{ExtensionApplyRequest, ExtensionError, JOURNAL_FILE_NAME};
use crate::extensions::recovery::{undo_step, JournalEntry};
use crate::io::SwitchIo;

/// Rolls an operation back and makes that completion durable. A marker
/// failure is recoverable because `undo_step` recognizes already-restored
/// state; it is still reported as recovery-required rather than hidden.
pub(super) fn failed_after_rollback<Io: SwitchIo>(
    io: &Io,
    request: &ExtensionApplyRequest<'_>,
    record: &mut ExtensionOperationRecord,
    completed: &[AppliedExtensionStep],
    failed_slot: Option<(usize, usize)>,
    message: String,
) -> ExtensionError {
    let rollback = rollback_completed(io, completed);
    if !rollback.failed.is_empty() {
        return ExtensionError::RecoveryRequired {
            message: format!(
                "操作失败且回滚未完成：{}；相关目标已阻断写入，请按恢复报告处理",
                rollback.failed.join("；")
            ),
        };
    }
    record.rollback = Some(rollback.clone());
    for (resource_index, resource) in record.resources.iter_mut().enumerate() {
        for (index, target_result) in resource.targets.iter_mut().enumerate() {
            if Some((resource_index, index)) != failed_slot
                && matches!(target_result.outcome, TargetOutcome::Applied)
            {
                target_result.outcome = TargetOutcome::Restored {
                    message: "失败后已恢复原内容".to_string(),
                };
            }
        }
    }
    let journal_path = request.journal_dir.join(JOURNAL_FILE_NAME);
    if let Err(error) = append_journal(
        io,
        &journal_path,
        &JournalEntry::RolledBack {
            operation_id: request.operation_id.to_string(),
        },
    ) {
        return ExtensionError::RecoveryRequired {
            message: format!("客户端变更已回滚，但无法记录回滚完成：{error}"),
        };
    }
    if let Err(error) = io.remove_dir_all(request.journal_dir) {
        return ExtensionError::RecoveryRequired {
            message: format!(
                "客户端变更已回滚，但无法清理事务标记：{error}；请使用恢复入口完成清理"
            ),
        };
    }
    ExtensionError::Failed {
        message,
        record: record.clone(),
        rollback,
    }
}

/// A library compensation failed after client changes had already been
/// written. The client files are restored immediately, but this deliberately
/// does not append `RolledBack`: the journal's `LibraryPrepared` payload is
/// still needed to repair the library during explicit recovery.
pub(super) fn recovery_required_after_client_rollback<Io: SwitchIo>(
    io: &Io,
    record: &mut ExtensionOperationRecord,
    completed: &[AppliedExtensionStep],
    message: String,
) -> ExtensionError {
    let rollback = rollback_completed(io, completed);
    if !rollback.failed.is_empty() {
        return ExtensionError::RecoveryRequired {
            message: format!(
                "{message}；客户端回滚也未完成：{}",
                rollback.failed.join("；")
            ),
        };
    }
    record.rollback = Some(rollback);
    ExtensionError::RecoveryRequired { message }
}

fn rollback_completed<Io: SwitchIo>(
    io: &Io,
    completed: &[AppliedExtensionStep],
) -> RollbackSummary {
    let mut restored = Vec::new();
    let mut failed = Vec::new();
    for step in completed.iter().rev() {
        match undo_step(io, &step.step) {
            Ok(message) => restored.push(message),
            Err(message) => failed.push(message),
        }
    }
    RollbackSummary { restored, failed }
}
