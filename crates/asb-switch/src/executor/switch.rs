//! The locked single-file transactions: provider projection and client-file
//! restore share one lock, backup, and rollback discipline.

use asb_core::{adapter, AppKind, BackupRecord, LockStatus, SwitchPlan, SwitchPreview};
use std::path::Path;

use super::preview::read_current_or_empty;
use super::{
    sha256_hex, timestamp_name, write_backup_metadata, RecoveryOutcome, RestoreOutcome,
    SwitchError, SwitchOutcome, SwitchRequest, PROCESS_NAME,
};
#[path = "config_file.rs"]
mod config_file;
pub(crate) use config_file::{back_up_current, commit_rendered};

use crate::codex_auth::{commit_auth, restore_pair};
use crate::io::SwitchIo;
use crate::lockfile::{self, AcquireOutcome};
use crate::restore::{restore_backup_content, restore_locked};

/// Executes one provider projection transactionally. A successful client
/// write is not complete until `commit` records the same fact in application
/// state; if that commit fails, this function restores the just-created
/// backup before releasing the lock. There is deliberately no no-commit
/// execution entry point.
pub fn execute<Io: SwitchIo, Commit>(
    io: &Io,
    req: &SwitchRequest,
    commit: Commit,
) -> Result<SwitchOutcome, SwitchError>
where
    Commit: FnOnce(&SwitchOutcome) -> Result<(), String>,
{
    execute_with_auth(io, req, false, None, None, None, commit)
}

/// Executes a Codex projection and optionally commits its paired auth.json
/// API-key patch. The extra hashes are private execution inputs from preview.
pub fn execute_codex<Io: SwitchIo, Commit>(
    io: &Io,
    req: &SwitchRequest,
    expected_auth_hash: Option<&str>,
    expected_auth_existed: Option<bool>,
    expected_auth_rendered_hash: Option<&str>,
    commit: Commit,
) -> Result<SwitchOutcome, SwitchError>
where
    Commit: FnOnce(&SwitchOutcome) -> Result<(), String>,
{
    execute_with_auth(
        io,
        req,
        true,
        expected_auth_hash,
        expected_auth_existed,
        expected_auth_rendered_hash,
        commit,
    )
}

fn execute_with_auth<Io: SwitchIo, Commit>(
    io: &Io,
    req: &SwitchRequest,
    auth_transaction: bool,
    expected_auth_hash: Option<&str>,
    expected_auth_existed: Option<bool>,
    expected_auth_rendered_hash: Option<&str>,
    commit: Commit,
) -> Result<SwitchOutcome, SwitchError>
where
    Commit: FnOnce(&SwitchOutcome) -> Result<(), String>,
{
    crate::config_journal::require_clear(io, req.backup_dir, req.plan.app())?;
    if let Some(parent) = req.target.parent() {
        io.ensure_dir(parent)
            .map_err(|error| SwitchError::CommitFailed {
                stage: "target-dir",
                message: error.to_string(),
                recovery: RecoveryOutcome::NotNeeded,
            })?;
    }
    match lockfile::acquire(io, req.target, PROCESS_NAME) {
        AcquireOutcome::Acquired => {}
        AcquireOutcome::Busy(status) => return Err(SwitchError::BlockedByLock { status }),
    }
    execute_locked(
        io,
        req,
        auth_transaction,
        expected_auth_hash,
        expected_auth_existed,
        expected_auth_rendered_hash,
        commit,
    )
}

/// Restores one recorded client configuration backup through the same locked
/// transaction boundary as provider projection. The caller must commit the
/// corresponding application write record before this function releases the
/// lock; a commit failure restores the pre-restore snapshot.
pub fn restore<Io: SwitchIo, Commit>(
    io: &Io,
    backup: &BackupRecord,
    target: &Path,
    commit: Commit,
) -> Result<RestoreOutcome, SwitchError>
where
    Commit: FnOnce(&RestoreOutcome) -> Result<(), String>,
{
    restore_projected(io, backup, target, None, None, commit)
}

/// Restores a verified backup with a current typed endpoint projection.
/// The original snapshot remains immutable and is always checked before this candidate.
pub fn restore_projected<Io: SwitchIo, Commit>(
    io: &Io,
    backup: &BackupRecord,
    target: &Path,
    projected: Option<&str>,
    expected: Option<&super::RestoreExpectation>,
    commit: Commit,
) -> Result<RestoreOutcome, SwitchError>
where
    Commit: FnOnce(&RestoreOutcome) -> Result<(), String>,
{
    let directory = Path::new(&backup.backup_path)
        .parent()
        .expect("backup directory");
    crate::config_journal::require_clear(io, directory, backup.app)?;
    if let Some(parent) = target.parent() {
        io.ensure_dir(parent)
            .map_err(|error| SwitchError::CommitFailed {
                stage: "target-dir",
                message: error.to_string(),
                recovery: RecoveryOutcome::NotNeeded,
            })?;
    }
    match lockfile::acquire(io, target, PROCESS_NAME) {
        AcquireOutcome::Busy(status) => Err(SwitchError::BlockedByLock { status }),
        AcquireOutcome::Acquired => {
            let result = restore_locked(io, backup, target, projected, expected, commit);
            match lockfile::release(io, target) {
                Ok(()) => result,
                Err(message) => match result {
                    Ok(mut outcome) => {
                        outcome.warnings.push(asb_core::contracts::LocalizedMessage::new(
                            "warnings.lockReleaseAfterRestore",
                            serde_json::json!({ "detail": message }),
                            format!("配置已恢复，但写入锁释放失败：{message}"),
                        ));
                        Ok(outcome)
                    }
                    Err(prior) => Err(SwitchError::LockReleaseFailed {
                        message,
                        prior: Box::new(prior),
                    }),
                },
            }
        }
    }
}

/// Reads the live file and refuses to continue when its hash no longer
/// matches the previewed state. Returns the text, whether the target
/// existed, and the verified hash.
pub(crate) fn read_unchanged_current<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    app: AppKind,
    expected_hash: &str,
) -> Result<(String, bool, String), SwitchError> {
    let (current, target_existed) =
        read_current_or_empty(io, target, app).map_err(|e| SwitchError::ReadCurrent {
            message: e.to_string(),
        })?;
    let found_hash = sha256_hex(&current);
    if found_hash != expected_hash {
        return Err(SwitchError::ExternalChange {
            expected_hash: expected_hash.to_string(),
            found_hash,
        });
    }
    Ok((current, target_existed, found_hash))
}

/// Re-reads the target at the last recoverable point before a mutation. The
/// comparison includes existence because an empty document and a missing file
/// have the same content hash but different restoration semantics.
pub(crate) fn verify_live_snapshot<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    app: AppKind,
    expected_content: &str,
    expected_existed: bool,
) -> Result<(), SwitchError> {
    let (found_content, found_existed) =
        read_current_or_empty(io, target, app).map_err(|error| SwitchError::ReadCurrent {
            message: error.to_string(),
        })?;
    if found_content == expected_content && found_existed == expected_existed {
        return Ok(());
    }
    Err(SwitchError::ExternalChange {
        expected_hash: sha256_hex(expected_content),
        found_hash: sha256_hex(&found_content),
    })
}

/// Produces the preview and the exact private candidate rendering, refusing
/// a plan whose rendering changed after the user saw the preview.
fn plan_candidate(
    plan: &SwitchPlan,
    current: &str,
    backup_dir: &str,
    expected_rendered_hash: &str,
) -> Result<(SwitchPreview, String), SwitchError> {
    let preview =
        adapter::preview(current, plan, backup_dir).map_err(super::preview::plan_rejected)?;
    let rendered = adapter::render(current, plan).map_err(super::preview::plan_rejected)?;
    if sha256_hex(&rendered) != expected_rendered_hash {
        return Err(SwitchError::PlanChanged);
    }
    Ok((preview, rendered))
}

/// Snapshots the current content as the pre-write backup, sidecar metadata
/// included.
/// The locked body of one transaction: verify → plan → re-verify → backup →
/// commit. Every exit path releases the lock through `finish`.
struct PreparedSwitch {
    current: String,
    preview: SwitchPreview,
    rendered: String,
    backup: BackupRecord,
    auth: Option<crate::codex_auth::AuthChange>,
    auth_backup: Option<BackupRecord>,
}
fn execute_locked<Io: SwitchIo, Commit>(
    io: &Io,
    request: &SwitchRequest,
    auth_transaction: bool,
    expected_auth_hash: Option<&str>,
    expected_auth_existed: Option<bool>,
    expected_auth_rendered_hash: Option<&str>,
    commit: Commit,
) -> Result<SwitchOutcome, SwitchError>
where
    Commit: FnOnce(&SwitchOutcome) -> Result<(), String>,
{
    let result = prepare_switch(
        io,
        request,
        auth_transaction,
        expected_auth_hash,
        expected_auth_existed,
        expected_auth_rendered_hash,
    )
    .and_then(|prepared| commit_switch(io, request, prepared, commit));
    finish_execution(io, request.target, result)
}
fn prepare_switch<Io: SwitchIo>(
    io: &Io,
    request: &SwitchRequest,
    auth_transaction: bool,
    expected_auth_hash: Option<&str>,
    expected_auth_existed: Option<bool>,
    expected_auth_rendered_hash: Option<&str>,
) -> Result<PreparedSwitch, SwitchError> {
    let (current, existed, hash) = read_unchanged_current(
        io,
        request.target,
        request.plan.app(),
        request.expected_hash,
    )?;
    let (preview, rendered) = plan_candidate(
        request.plan,
        &current,
        &request.backup_dir.to_string_lossy(),
        request.expected_rendered_hash,
    )?;
    let auth = if auth_transaction {
        crate::codex_auth::validate_storage(&current, request.plan)?;
        crate::codex_auth::prepare(
            io,
            request.target,
            crate::codex_auth::expected_action(request.plan),
        )?
    } else {
        None
    };
    validate_auth_preview(
        auth.as_ref(),
        expected_auth_hash,
        expected_auth_existed,
        expected_auth_rendered_hash,
    )?;
    let backup = back_up_current(
        io,
        request.target,
        request.backup_dir,
        &current,
        &hash,
        existed,
        request.plan.app(),
        "provider-projection",
    )?;
    let auth_backup = auth
        .as_ref()
        .map(|change| {
            crate::codex_auth::backup(io, change, &backup, request.backup_dir, &timestamp_name(io))
        })
        .transpose()?;
    Ok(PreparedSwitch {
        current,
        preview,
        rendered,
        backup,
        auth,
        auth_backup,
    })
}
fn validate_auth_preview(
    change: Option<&crate::codex_auth::AuthChange>,
    before: Option<&str>,
    existed: Option<bool>,
    after: Option<&str>,
) -> Result<(), SwitchError> {
    match (change, before, existed, after) {
        (None, None, None, None) => Ok(()),
        (Some(change), Some(before), Some(existed), Some(after)) => {
            if change.before_hash != before || change.before_existed != existed {
                return Err(SwitchError::ExternalChange {
                    expected_hash: before.into(),
                    found_hash: change.before_hash.clone(),
                });
            }
            if change.after_hash != after {
                return Err(SwitchError::PlanChanged);
            }
            Ok(())
        }
        _ => Err(SwitchError::PlanChanged),
    }
}
fn switch_journal(request: &SwitchRequest, prepared: &PreparedSwitch) -> crate::PendingConfigWrite {
    let PreparedSwitch {
        backup,
        rendered,
        auth,
        auth_backup,
        ..
    } = prepared;
    crate::PendingConfigWrite {
        version: 1,
        app: request.plan.app(),
        profile_id: Some(request.plan.profile.id.clone()),
        backup: backup.clone(),
        after_hash: sha256_hex(&rendered),
        after_existed: true,
        auth: auth
            .as_ref()
            .zip(auth_backup.as_ref())
            .map(|(change, backup)| crate::PendingAuthWrite {
                backup: backup.clone(),
                after_hash: change.after_hash.clone(),
                after_existed: true,
            }),
    }
}

fn commit_switch<Io: SwitchIo, Commit>(
    io: &Io,
    request: &SwitchRequest,
    prepared: PreparedSwitch,
    commit: Commit,
) -> Result<SwitchOutcome, SwitchError>
where
    Commit: FnOnce(&SwitchOutcome) -> Result<(), String>,
{
    let pending = switch_journal(request, &prepared);
    let PreparedSwitch {
        current,
        preview,
        rendered,
        backup,
        auth,
        auth_backup,
    } = prepared;
    crate::config_journal::track(io, pending, || {
        commit_rendered(
            io,
            request.target,
            request.plan.app(),
            &rendered,
            &backup,
            &current,
        )?;
        if let (Some(change), Some(auth_backup)) = (&auth, &auth_backup) {
            if let Err(error) = commit_auth(io, change) {
                let recovery = restore_pair(
                    io,
                    request.target,
                    &backup,
                    &rendered,
                    Some((change, auth_backup)),
                );
                return Err(with_recovery("auth-write", error, recovery));
            }
        }
        let outcome = SwitchOutcome {
            lock: LockStatus::Free,
            acquired_at: backup.created_at.clone(),
            changed: changed_paths(request.target, auth.as_ref()),
            warnings: preview.warnings.clone(),
            backup,
            preview,
            recovery: RecoveryOutcome::NotNeeded,
            final_hash: sha256_hex(&rendered),
        };
        if let Err(message) = commit(&outcome) {
            let recovery = restore_pair(
                io,
                request.target,
                &outcome.backup,
                &rendered,
                auth.as_ref().zip(auth_backup.as_ref()),
            );
            return Err(SwitchError::CommitFailed {
                stage: "state-save",
                message,
                recovery,
            });
        }
        Ok(outcome)
    })
}

fn changed_paths(target: &Path, auth: Option<&crate::codex_auth::AuthChange>) -> Vec<String> {
    let mut paths = vec![target.to_string_lossy().into_owned()];
    if let Some(auth) = auth {
        paths.push(auth.target.to_string_lossy().into_owned());
    }
    paths
}

fn with_recovery(
    stage: &'static str,
    error: SwitchError,
    recovery: RecoveryOutcome,
) -> SwitchError {
    SwitchError::CommitFailed {
        stage,
        message: error.to_string(),
        recovery,
    }
}

fn finish_execution<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    result: Result<SwitchOutcome, SwitchError>,
) -> Result<SwitchOutcome, SwitchError> {
    match lockfile::release(io, target) {
        Ok(()) => result,
        Err(message) => match result {
            Ok(mut outcome) => {
                outcome.warnings.push(asb_core::contracts::LocalizedMessage::new(
                    "warnings.lockReleaseAfterWrite",
                    serde_json::json!({ "detail": message }),
                    format!("写入已完成，但无法释放写入锁：{message}"),
                ));
                Ok(outcome)
            }
            Err(prior) => Err(SwitchError::LockReleaseFailed {
                message,
                prior: Box::new(prior),
            }),
        },
    }
}
