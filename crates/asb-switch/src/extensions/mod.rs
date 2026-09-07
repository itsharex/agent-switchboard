//! Extensions transaction layer: managed directory deployment, document
//! patches, journaling, rollback, and crash recovery for the extensions
//! workspace.
//!
//! The executor in [`crate::executor`] owns provider switching; this module
//! extends the same locking, backup, and rollback discipline to extension
//! operations. A batch is a *resilient transaction*, not a filesystem
//! transaction: every step is journaled before and after, a failure rolls
//! the whole approved batch back in reverse order, and a crash leaves a
//! journal that [`recovery`] can finish.

pub mod recovery;
pub mod transaction;

pub use recovery::{complete_recovery, journal_lock_targets, recover_pending, RecoveryReport};
pub use transaction::{
    apply_extension_plan, walk_content_entries, ExtensionApplyRequest, ExtensionError,
    LibraryCommitError,
};
