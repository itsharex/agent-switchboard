//! Pending-journal recovery: parsing, lock-target reconstruction, and the
//! reverse replay that finishes an interrupted operation.

use std::path::{Path, PathBuf};

use super::undo::undo_step;
use super::{JournalEntry, RecoveryReport, StepKind};
use crate::extensions::transaction::JOURNAL_FILE_NAME;
use crate::io::{PathKind, SwitchIo};
use asb_core::extensions::contracts::AppliedStep;

/// Reads the journal in `journal_dir`, if any, and finishes the operation:
/// committed operations are cleaned up, incomplete ones are rolled back in
/// reverse. Returns `None` when there is no pending journal. When undo
/// fails, the journal and backups are left in place and the failed entries
/// name the targets that need manual attention.
pub fn recover_pending<Io: SwitchIo>(io: &Io, journal_dir: &Path) -> Option<RecoveryReport> {
    let journal_path = journal_dir.join(JOURNAL_FILE_NAME);
    let bytes = io.read_bytes(&journal_path).ok()?;
    let entries = parse_journal(&bytes);
    let Some(first) = entries.first() else {
        return Some(RecoveryReport {
            operation_id: journal_dir
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
            completed_cleanly: true,
            restored: Vec::new(),
            failed: vec!["事务日志为空或损坏；已保留目录供检查".to_string()],
            library_recovery: None,
        });
    };
    let operation_id = match first {
        JournalEntry::Prepared { operation_id, .. } => operation_id.clone(),
        _ => String::new(),
    };
    let terminal = entries.iter().any(|entry| {
        matches!(
            entry,
            JournalEntry::Committed { .. } | JournalEntry::RolledBack { .. }
        )
    });

    let mut report = RecoveryReport {
        operation_id,
        completed_cleanly: terminal,
        restored: Vec::new(),
        failed: Vec::new(),
        library_recovery: None,
    };

    if !terminal {
        // The library was about to change (or half-changed): hand the
        // journaled pre/post payload to the caller for restoration.
        if let Some(JournalEntry::LibraryPrepared { commit, .. }) = entries
            .iter()
            .rev()
            .find(|entry| matches!(entry, JournalEntry::LibraryPrepared { .. }))
        {
            report.library_recovery = Some(commit.clone());
        }
        // Undo completed steps in reverse, then any in-flight step whose
        // start was recorded without a matching completion.
        let mut undo_queue: Vec<AppliedStep> = Vec::new();
        for entry in &entries {
            match entry {
                JournalEntry::StepCompleted { detail, .. } => undo_queue.push(detail.clone()),
                _ => {}
            }
        }
        // Started-but-not-completed steps, in start order, undone after
        // the completed ones (reverse replay covers them last).
        for entry in &entries {
            if let JournalEntry::StepStarted {
                target,
                index,
                undo: Some(undo),
                ..
            } = entry
            {
                let paired = entries.iter().any(|other| {
                    matches!(other,
                        JournalEntry::StepCompleted { target: t, index: i, .. }
                        if t == target && i == index)
                });
                if !paired {
                    undo_queue.push(undo.clone());
                }
            }
        }
        for step in undo_queue.iter().rev() {
            match undo_step(io, step) {
                Ok(message) => report.restored.push(message),
                Err(message) => report.failed.push(message),
            }
        }
        // Deploy steps carry no pre-recorded undo: sweep the swap
        // artifacts of any started-but-not-completed deploy.
        for entry in &entries {
            if let JournalEntry::StepStarted {
                kind: StepKind::Deploy,
                path,
                ..
            } = entry
            {
                sweep_in_flight(io, path, &mut report);
            }
        }
    }

    // A journal carrying a library pre-state stays until the caller has
    // restored that state too. It is the only durable copy of the recovery
    // contract when a process died during the library commit.
    if report.failed.is_empty() && report.library_recovery.is_none() {
        let _ = io.remove_dir_all(journal_dir);
    }
    Some(report)
}

/// Removes a journal only after the caller has restored its associated
/// library pre-state. Client recovery alone is intentionally insufficient.
pub fn complete_recovery<Io: SwitchIo>(io: &Io, journal_dir: &Path) -> Result<(), String> {
    io.remove_dir_all(journal_dir)
        .map_err(|error| error.to_string())
}

/// Rebuilds the sorted, deduplicated lock-target list of the journaled
/// plan, so callers can take the same locks the executor would before
/// running recovery.
pub fn journal_lock_targets<Io: SwitchIo>(io: &Io, journal_dir: &Path) -> Vec<PathBuf> {
    let journal_path = journal_dir.join(JOURNAL_FILE_NAME);
    let Ok(bytes) = io.read_bytes(&journal_path) else {
        return Vec::new();
    };
    let entries = parse_journal(&bytes);
    let mut targets = Vec::new();
    for entry in &entries {
        if let JournalEntry::Prepared { plan, .. } = entry {
            if let Ok(plan) =
                serde_json::from_value::<asb_core::extensions::plan::ExtensionPlan>(plan.clone())
            {
                for target in plan.planned_targets() {
                    for step in &target.steps {
                        match step {
                            asb_core::extensions::plan::PlanStep::DocumentWrite {
                                path, ..
                            }
                            | asb_core::extensions::plan::PlanStep::DocumentRemove {
                                path, ..
                            } => {
                                targets.push(PathBuf::from(path));
                            }
                            asb_core::extensions::plan::PlanStep::DirectoryDeploy {
                                target_dir,
                                ..
                            }
                            | asb_core::extensions::plan::PlanStep::DirectoryRemove {
                                target_dir,
                                ..
                            } => {
                                targets.push(PathBuf::from(target_dir));
                            }
                        }
                    }
                }
            }
            break;
        }
    }
    // A torn or manually damaged Prepared payload must not make recovery
    // run without locks when later journal records still identify files that
    // may need undoing. Those records are the exact paths the executor
    // already touched, so use them only as a recovery fallback.
    if targets.is_empty() {
        for entry in entries {
            match entry {
                JournalEntry::StepStarted { path, .. } => targets.push(PathBuf::from(path)),
                JournalEntry::StepCompleted { detail, .. } => match detail {
                    AppliedStep::DocumentWritten { path, .. }
                    | AppliedStep::DocumentRemoved { path, .. } => {
                        targets.push(PathBuf::from(path))
                    }
                    AppliedStep::DirectoryDeployed { target_dir, .. }
                    | AppliedStep::DirectoryRemoved { target_dir, .. } => {
                        targets.push(PathBuf::from(target_dir));
                    }
                },
                _ => {}
            }
        }
    }
    targets.sort();
    targets.dedup();
    targets
}

fn parse_journal(bytes: &[u8]) -> Vec<JournalEntry> {
    let text = String::from_utf8_lossy(bytes);
    let mut entries = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        // A torn final line (crash mid-append) is skipped; a corrupted
        // middle line aborts parsing and keeps earlier entries.
        match serde_json::from_str::<JournalEntry>(line) {
            Ok(entry) => entries.push(entry),
            Err(_) => break,
        }
    }
    entries
}

/// Removes staging leftovers and finishes a half-done directory swap:
/// the old tree comes back, the new tree goes away.
fn sweep_in_flight<Io: SwitchIo>(io: &Io, path: &str, report: &mut RecoveryReport) {
    let target = PathBuf::from(path);
    let file_name = target
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent = match target.parent() {
        Some(parent) => parent.to_path_buf(),
        None => return,
    };
    if let Ok(children) = io.list_dir(&parent) {
        let suffix = |name: &str, tail: &str| name.starts_with(&file_name) && name.ends_with(tail);
        let mut new_dir = None;
        let mut old_dir = None;
        for child in children {
            let name = child
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            if suffix(&name, ".asb-ext-new") {
                new_dir = Some(child);
            } else if suffix(&name, ".asb-ext-old") {
                old_dir = Some(child);
            }
        }
        if matches!(io.path_kind(&target), Ok(PathKind::Absent)) {
            if let Some(old_path) = old_dir.take() {
                if io.rename(&old_path, &target).is_ok() {
                    report
                        .restored
                        .push(format!("已恢复 {}（中断的目录切换）", target.display()));
                } else {
                    report
                        .failed
                        .push(format!("无法恢复中断的目录切换：{}", target.display()));
                }
            }
        }
        if let Some(new_path) = new_dir.take() {
            let _ = io.remove_dir_all(&new_path);
        }
    }
    // Leftover staging temporaries of an in-flight document write.
    if let Ok(children) = io.list_dir(&parent) {
        for child in children {
            let name = child
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.starts_with(&file_name) && name.ends_with(".asb-ext-tmp") {
                let _ = io.remove(&child);
            }
        }
    }
}
