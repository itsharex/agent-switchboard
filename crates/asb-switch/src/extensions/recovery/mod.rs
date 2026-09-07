//! Journal format, step undo, and startup recovery for extension
//! transactions.
//!
//! The journal is an append-only JSON-lines file inside the operation's
//! transaction directory. Every record is one complete, newline-terminated
//! line. Before a visible change runs, its `StepStarted` record already
//! carries the complete undo information (backup location, pre-state
//! digest, expected post-state digest), so the window between a replaced
//! file and its completion record stays recoverable. Recovery replays undo
//! in reverse order, refusing to overwrite anything that is not exactly
//! what the transaction last wrote or observed.

mod pending;
mod undo;

pub use pending::{complete_recovery, journal_lock_targets, recover_pending};
pub(crate) use undo::undo_step;

use serde::{Deserialize, Serialize};

use asb_core::extensions::contracts::AppliedStep;

/// One journal record. `Prepared` is always the first line and is written
/// before any lock or mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "phase",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum JournalEntry {
    Prepared {
        operation_id: String,
        created_at: String,
        /// Full plan snapshot (payload included); restricted backend-local
        /// storage only.
        plan: serde_json::Value,
    },
    StepStarted {
        target: usize,
        index: usize,
        kind: StepKind,
        path: String,
        /// Complete undo information prepared before the step runs. When
        /// a crash lands between the visible change and the completion
        /// record, this is what recovery undoes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        undo: Option<AppliedStep>,
    },
    StepCompleted {
        target: usize,
        index: usize,
        detail: AppliedStep,
    },
    /// The library change the commit closure was about to apply. A crash
    /// between this record and `Committed` means the library may be
    /// half-written; the caller restores it from the payload's pre-state.
    LibraryPrepared {
        operation_id: String,
        commit: serde_json::Value,
    },
    Committed {
        operation_id: String,
    },
    /// The executor finished reversing a failed operation. Recovery only
    /// removes this terminal journal; it must never run undo a second time.
    RolledBack {
        operation_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepKind {
    Document,
    Deploy,
    Remove,
}

/// The outcome of examining and finishing one pending journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReport {
    pub operation_id: String,
    /// The journal represented a finished, committed operation; only
    /// cleanup was needed.
    pub completed_cleanly: bool,
    pub restored: Vec<String>,
    pub failed: Vec<String>,
    /// The journaled library change of an uncommitted operation, for the
    /// caller to restore the library to its pre-operation state. Present
    /// when `LibraryPrepared` was written but `Committed` was not.
    pub library_recovery: Option<serde_json::Value>,
}
