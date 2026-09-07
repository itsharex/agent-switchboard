//! The batch pipeline: journal bootstrap, locking, step orchestration, and
//! the library commit inside the journal boundary.

use std::path::{Path, PathBuf};

use asb_core::extensions::contracts::{
    AppliedExtensionStep, ExtensionOperationRecord, ExtensionTargetResult, OperationResourceRecord,
    TargetOutcome,
};
use asb_core::extensions::plan::{ExtensionPlan, PlanStep};

use super::prepare::{prepare_step, StepFailure};
use super::run::run_prepared_step;
use super::undo::{failed_after_rollback, recovery_required_after_client_rollback};
use super::verify::verify_plan_freshness;
use super::{ExtensionApplyRequest, ExtensionError, LibraryCommitError, JOURNAL_FILE_NAME};
use crate::executor::PROCESS_NAME;
use crate::extensions::recovery::JournalEntry;
use crate::io::SwitchIo;
use crate::lockfile;

// ---------------------------------------------------------------- locking

fn collect_lock_targets(plan: &ExtensionPlan) -> Vec<PathBuf> {
    let mut targets: Vec<PathBuf> = Vec::new();
    for target in plan.planned_targets() {
        for step in &target.steps {
            match step {
                PlanStep::DocumentWrite { path, .. } | PlanStep::DocumentRemove { path, .. } => {
                    targets.push(PathBuf::from(path));
                }
                PlanStep::DirectoryDeploy { target_dir, .. }
                | PlanStep::DirectoryRemove { target_dir, .. } => {
                    targets.push(PathBuf::from(target_dir));
                }
            }
        }
    }
    targets.sort();
    targets.dedup();
    targets
}

fn release_locks<Io: SwitchIo>(io: &Io, held_locks: &[PathBuf]) {
    for lock in held_locks.iter().rev() {
        let _ = lockfile::release(io, lock);
    }
}

// ---------------------------------------------------------------- the pipeline

/// A whole-batch failure with the stale marker and the failing target's
/// slot in the record, for outcome bookkeeping after rollback. The slot is
/// (resource index, target index within that resource).
struct BatchFailure {
    stale: bool,
    message: String,
    failed_target_index: Option<(usize, usize)>,
}

/// Records one failed target row and yields the matching batch failure.
fn batch_failure(
    record: &mut ExtensionOperationRecord,
    resource_index: usize,
    target: &asb_core::extensions::contracts::ExtensionTarget,
    failure: StepFailure,
) -> BatchFailure {
    batch_failure_message(
        record,
        resource_index,
        target,
        failure.error.message().to_string(),
        failure.error.is_stale(),
    )
}

fn batch_failure_message(
    record: &mut ExtensionOperationRecord,
    resource_index: usize,
    target: &asb_core::extensions::contracts::ExtensionTarget,
    message: String,
    stale: bool,
) -> BatchFailure {
    let rows = &mut record.resources[resource_index].targets;
    rows.push(ExtensionTargetResult {
        target: target.clone(),
        outcome: TargetOutcome::Failed {
            message: message.clone(),
        },
        warnings: Vec::new(),
    });
    BatchFailure {
        stale,
        message,
        failed_target_index: Some((resource_index, rows.len() - 1)),
    }
}

/// Executes the plan's targets in order and commits the library inside the
/// journal boundary, rolling the whole batch back on any failure. The plan
/// itself is the only write authorization; every expectation is
/// re-verified under lock.
pub fn apply_extension_plan<Io, Commit>(
    io: &Io,
    request: &ExtensionApplyRequest<'_>,
    commit: Commit,
) -> Result<ExtensionOperationRecord, ExtensionError>
where
    Io: SwitchIo,
    Commit: FnOnce(
        &ExtensionOperationRecord,
        &[AppliedExtensionStep],
    ) -> Result<(), LibraryCommitError>,
{
    let plan = request.plan;

    // Journal first: before any mutation, so a crash is always explainable.
    // Every record is a complete line and is flushed.
    io.ensure_dir(request.journal_dir).map_err(|error| {
        ExtensionError::preparation(format!(
            "无法创建事务目录 {}：{error}",
            request.journal_dir.display()
        ))
    })?;
    let journal_path = request.journal_dir.join(JOURNAL_FILE_NAME);
    // A journal for this operation id can only come from an earlier attempt
    // of the same operation that wrote nothing; attempts use fresh ids, so
    // rewriting it is safe and a crashed operation never collides here.
    let _ = io.remove(&journal_path);
    let prepared = serde_json::to_vec(&JournalEntry::Prepared {
        operation_id: request.operation_id.to_string(),
        created_at: io.now_rfc3339(),
        plan: serde_json::to_value(plan)
            .map_err(|error| ExtensionError::preparation(format!("计划无法序列化：{error}")))?,
    })
    .map_err(|error| ExtensionError::preparation(format!("日志无法序列化：{error}")))?;
    write_journal_line(io, &journal_path, &prepared)?;

    // Acquire all target locks in sorted order, releasing on failure. A
    // lock file sits beside its target, so the target's parent directory
    // must exist first — creating the parent never touches the target
    // itself and matches what the clients' own installers guarantee.
    let lock_targets = collect_lock_targets(plan);
    for target in &lock_targets {
        if let Some(parent) = target.parent() {
            io.ensure_dir(parent).map_err(|error| {
                ExtensionError::preparation(format!(
                    "无法创建目标父目录 {}：{error}",
                    parent.display()
                ))
            })?;
        }
    }
    let mut held_locks: Vec<PathBuf> = Vec::new();
    for target in &lock_targets {
        let outcome = lockfile::acquire(io, target, PROCESS_NAME);
        match outcome {
            lockfile::AcquireOutcome::Acquired => held_locks.push(target.clone()),
            lockfile::AcquireOutcome::Busy(status) => {
                for held in held_locks.iter().rev() {
                    let _ = lockfile::release(io, held);
                }
                return Err(ExtensionError::Blocked {
                    message: format!(
                        "目标 {} 正在被占用（{status:?}）；请稍后重试或处理既有锁",
                        target.display(),
                    ),
                });
            }
        }
    }

    let mut record = ExtensionOperationRecord {
        schema_version: asb_core::extensions::contracts::EXTENSIONS_SCHEMA_VERSION,
        id: request.operation_id.to_string(),
        created_at: io.now_rfc3339(),
        finished_at: None,
        resources: plan
            .operations
            .iter()
            .map(|operation| OperationResourceRecord {
                definition_id: operation.definition_id.clone(),
                definition_revision: operation.definition_revision,
                operation: operation.operation,
                targets: Vec::new(),
            })
            .collect(),
        rollback: None,
    };

    // Under lock, before any write: the plan must still be inside its
    // validity window. The generation and secret digests are re-verified by
    // the commanding layer, which owns the store and the credential vault.
    if let Err(message) = verify_plan_freshness(io, plan) {
        for lock in held_locks.iter().rev() {
            let _ = lockfile::release(io, lock);
        }
        let _ = io.remove_dir_all(request.journal_dir);
        return Err(ExtensionError::PlanStale { message });
    }

    let mut completed: Vec<AppliedExtensionStep> = Vec::new();
    let result = execute_steps(io, request, &mut completed, &mut record);

    match result {
        Ok(()) => {
            record.finished_at = Some(io.now_rfc3339());
            // Journal the prepared library change before running the commit
            // closure: a crash during the library batch is recoverable to
            // the pre-operation state (client files plus library).
            if let Some(library_commit) = &request.library_commit {
                if let Err(error) = append_journal(
                    io,
                    &journal_path,
                    &JournalEntry::LibraryPrepared {
                        operation_id: request.operation_id.to_string(),
                        commit: library_commit.clone(),
                    },
                ) {
                    let failure = failed_after_rollback(
                        io,
                        request,
                        &mut record,
                        &completed,
                        None,
                        format!("无法写入扩展库事务日志：{error}"),
                    );
                    release_locks(io, &held_locks);
                    return Err(failure);
                }
            }
            match commit(&record, &completed) {
                Ok(()) => {
                    if append_journal(
                        io,
                        &journal_path,
                        &JournalEntry::Committed {
                            operation_id: request.operation_id.to_string(),
                        },
                    )
                    .is_err()
                    {
                        // The library commit's durable marker is written by
                        // its owner before this journal line. Recovery can
                        // therefore recognize this as a completed batch and
                        // clean the journal without undoing client files.
                        release_locks(io, &held_locks);
                        return Err(ExtensionError::RecoveryRequired {
                            message: format!(
                                "操作 {} 已完成持久化提交，但事务完成标记写入失败；请用恢复入口完成清理",
                                request.operation_id
                            ),
                        });
                    }
                    let _ = io.remove_dir_all(request.journal_dir);
                    release_locks(io, &held_locks);
                    Ok(record)
                }
                Err(LibraryCommitError::Failed { message }) => {
                    let failure = failed_after_rollback(
                        io,
                        request,
                        &mut record,
                        &completed,
                        None,
                        format!("扩展库提交失败：{message}"),
                    );
                    release_locks(io, &held_locks);
                    Err(failure)
                }
                Err(LibraryCommitError::RecoveryRequired { message }) => {
                    let failure = recovery_required_after_client_rollback(
                        io,
                        &mut record,
                        &completed,
                        format!("扩展库提交未能完整恢复：{message}"),
                    );
                    release_locks(io, &held_locks);
                    Err(failure)
                }
            }
        }
        Err(failure) => {
            if completed.is_empty() {
                let _ = io.remove_dir_all(request.journal_dir);
                release_locks(io, &held_locks);
                return if failure.stale {
                    Err(ExtensionError::PlanStale {
                        message: failure.message,
                    })
                } else {
                    Err(ExtensionError::preparation(failure.message))
                };
            }
            record.finished_at = Some(io.now_rfc3339());
            let failure = failed_after_rollback(
                io,
                request,
                &mut record,
                &completed,
                failure.failed_target_index,
                failure.message,
            );
            release_locks(io, &held_locks);
            Err(failure)
        }
    }
}

/// Executes every planned step. Each visible change enters `completed` the
/// moment it happens — before the journal completion record, before later
/// steps — so a failure anywhere rolls back every earlier write of the
/// batch, including writes of the same target.
fn execute_steps<Io: SwitchIo>(
    io: &Io,
    request: &ExtensionApplyRequest<'_>,
    completed: &mut Vec<AppliedExtensionStep>,
    record: &mut ExtensionOperationRecord,
) -> Result<(), BatchFailure> {
    let journal_path = request.journal_dir.join(JOURNAL_FILE_NAME);
    let plan = request.plan;
    // Flat target sequence number used by the journal's step entries; the
    // journal stays positional while the record groups by resource.
    let mut target_index = 0usize;
    for (resource_index, operation) in plan.operations.iter().enumerate() {
        for planned in &operation.targets {
            for (index, step) in planned.steps.iter().enumerate() {
                // Preparation: all checks, backups, and staging happen before
                // the step is announced; a preparation failure leaves nothing.
                let prepared = match prepare_step(io, step) {
                    Ok(prepared) => prepared,
                    Err(failure) => {
                        return Err(batch_failure(
                            record,
                            resource_index,
                            &planned.target,
                            failure,
                        ));
                    }
                };
                let (kind, path) = prepared.describe();
                if let Err(error) = append_journal(
                    io,
                    &journal_path,
                    &JournalEntry::StepStarted {
                        target: target_index,
                        index,
                        kind,
                        path,
                        undo: prepared.undo(),
                    },
                ) {
                    return Err(batch_failure_message(
                        record,
                        resource_index,
                        &planned.target,
                        format!("无法写入事务日志：{error}"),
                        false,
                    ));
                }
                match run_prepared_step(io, prepared) {
                    Ok(done) => {
                        let applied = AppliedExtensionStep {
                            resource_id: operation.definition_id.clone(),
                            target: planned.target.clone(),
                            step: done.clone(),
                        };
                        // A visible write enters the rollback set before the
                        // completion journal append, so that append failure is
                        // never allowed to strand the target.
                        completed.push(applied);
                        if let Err(error) = append_journal(
                            io,
                            &journal_path,
                            &JournalEntry::StepCompleted {
                                target: target_index,
                                index,
                                detail: done.clone(),
                            },
                        ) {
                            return Err(batch_failure_message(
                                record,
                                resource_index,
                                &planned.target,
                                format!("无法写入事务日志：{error}"),
                                false,
                            ));
                        }
                    }
                    Err(failure) => {
                        let StepFailure {
                            error,
                            completed: undo,
                        } = failure;
                        if let Some(done) = undo {
                            completed.push(AppliedExtensionStep {
                                resource_id: operation.definition_id.clone(),
                                target: planned.target.clone(),
                                step: done,
                            });
                        }
                        return Err(batch_failure(
                            record,
                            resource_index,
                            &planned.target,
                            StepFailure {
                                error,
                                completed: None,
                            },
                        ));
                    }
                }
            }
            // The binding commits only after every step of the target applied.
            // Detailed backup locations remain in the private operation
            // snapshot, never in this displayable record.
            record.resources[resource_index]
                .targets
                .push(ExtensionTargetResult {
                    target: planned.target.clone(),
                    outcome: TargetOutcome::Applied,
                    warnings: planned.warnings.clone(),
                });
            target_index += 1;
        }
    }
    Ok(())
}

pub(super) fn append_journal<Io: SwitchIo>(
    io: &Io,
    journal_path: &Path,
    entry: &JournalEntry,
) -> Result<(), String> {
    let mut line = serde_json::to_vec(entry).map_err(|error| error.to_string())?;
    line.push(b'\n');
    let existing = io.read_bytes(journal_path).unwrap_or_default();
    let mut combined = existing;
    combined.extend_from_slice(&line);
    io.write_bytes_replace(journal_path, &combined)
        .and_then(|()| io.sync_file(journal_path))
        .map_err(|error| error.to_string())
}

/// Writes the first journal line (create-new) and flushes it. The line is
/// always newline-terminated so the append-only parser stays sound.
fn write_journal_line<Io: SwitchIo>(
    io: &Io,
    journal_path: &Path,
    entry: &[u8],
) -> Result<(), ExtensionError> {
    let mut line = entry.to_vec();
    if !line.ends_with(b"\n") {
        line.push(b'\n');
    }
    io.write_new_bytes(journal_path, &line)
        .and_then(|()| io.sync_file(journal_path))
        .map_err(|error| {
            ExtensionError::preparation(format!(
                "无法写入事务日志 {}：{error}",
                journal_path.display()
            ))
        })
}
