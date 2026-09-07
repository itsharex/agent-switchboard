//! Transactional writes of a caller-rendered current client document.

use super::switch::{back_up_current, commit_rendered, read_unchanged_current};
use super::{
    sha256_hex, RecoveryOutcome, RenderedWriteOutcome, RenderedWriteRequest, SwitchError,
    PROCESS_NAME,
};
use crate::io::SwitchIo;
use crate::lockfile::{self, AcquireOutcome};
use crate::restore::restore_backup_content;

/// Applies a narrow rendered change through the same observable executor
/// boundary as a provider projection. The callback must persist the matching
/// application fact; a callback error restores the just-created backup.
pub fn execute_rendered<Io: SwitchIo, Commit>(
    io: &Io,
    req: &RenderedWriteRequest,
    commit: Commit,
) -> Result<RenderedWriteOutcome, SwitchError>
where
    Commit: FnOnce(&RenderedWriteOutcome) -> Result<(), String>,
{
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
    let result = execute_locked(io, req, commit);
    match lockfile::release(io, req.target) {
        Ok(()) => result,
        Err(message) => match result {
            Ok(mut outcome) => {
                outcome
                    .warnings
                    .push(format!("端点投影已完成，但无法释放写入锁：{message}"));
                Ok(outcome)
            }
            Err(prior) => Err(SwitchError::LockReleaseFailed {
                message,
                prior: Box::new(prior),
            }),
        },
    }
}

fn execute_locked<Io: SwitchIo, Commit>(
    io: &Io,
    req: &RenderedWriteRequest,
    commit: Commit,
) -> Result<RenderedWriteOutcome, SwitchError>
where
    Commit: FnOnce(&RenderedWriteOutcome) -> Result<(), String>,
{
    let (current, existed, current_hash) =
        read_unchanged_current(io, req.target, req.app, req.expected_hash)?;
    if existed != req.expected_target_existed {
        return Err(SwitchError::ExternalChange {
            expected_hash: req.expected_hash.to_string(),
            found_hash: current_hash,
        });
    }
    let backup = back_up_current(
        io,
        req.target,
        req.backup_dir,
        &current,
        req.expected_hash,
        existed,
        req.app,
        req.reason,
    )?;
    commit_rendered(io, req.target, req.app, req.rendered, &backup, &current)?;
    let outcome = RenderedWriteOutcome {
        backup,
        final_hash: sha256_hex(req.rendered),
        warnings: vec![],
    };
    if let Err(message) = commit(&outcome) {
        let recovery = restore_backup_content(io, req.target, &outcome.backup);
        return Err(SwitchError::CommitFailed {
            stage: "state-save",
            message,
            recovery,
        });
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::FsIo;
    use asb_core::{adapter, AppKind};

    #[test]
    fn endpoint_only_write_keeps_unowned_client_content_and_rolls_back_callback_failure() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("settings.json");
        let backup_dir = directory.path().join("backups");
        let current = r#"{"model":"user-selected","env":{"ANTHROPIC_BASE_URL":"http://127.0.0.1:47821","HOST_KEY":"keep"}}"#;
        std::fs::write(&target, current).unwrap();
        let rendered =
            adapter::render_gateway_base_url(AppKind::Claude, current, "http://127.0.0.1:47822")
                .unwrap();

        let outcome = execute_rendered(
            &FsIo,
            &RenderedWriteRequest {
                target: &target,
                app: AppKind::Claude,
                backup_dir: &backup_dir,
                expected_hash: &sha256_hex(current),
                expected_target_existed: true,
                rendered: &rendered,
                reason: "test-endpoint-only",
            },
            |_| Ok(()),
        )
        .unwrap();
        let written = std::fs::read_to_string(&target).unwrap();
        assert!(written.contains("user-selected"));
        assert!(written.contains("HOST_KEY"));
        assert_eq!(outcome.final_hash, sha256_hex(&written));

        let rollback_rendered =
            adapter::render_gateway_base_url(AppKind::Claude, &written, "http://127.0.0.1:47823")
                .unwrap();
        let error = execute_rendered(
            &FsIo,
            &RenderedWriteRequest {
                target: &target,
                app: AppKind::Claude,
                backup_dir: &backup_dir,
                expected_hash: &sha256_hex(&written),
                expected_target_existed: true,
                rendered: &rollback_rendered,
                reason: "test-endpoint-only",
            },
            |_| Err("state write rejected".to_string()),
        )
        .expect_err("callback failure restores the endpoint write");
        assert!(matches!(error, SwitchError::CommitFailed { .. }));
        assert_eq!(std::fs::read_to_string(target).unwrap(), written);
    }
}
