//! asb-switch is the switch executor and the only crate allowed to write
//! live configuration files.
//!
//! Planning lives in `asb-core` and is side-effect free; everything here that
//! touches a file is observable: locks are classified (free, held, stale,
//! indeterminate), backups carry hashes, and every failure reports whether
//! and how the live file was restored.

mod config_journal;
mod display;
pub mod executor;
pub mod extensions;
pub mod io;
pub mod lockfile;
pub mod pids;
mod prompt_documents;
mod restore;
mod subagent_settings;

pub use executor::{
    execute, execute_rendered, read_preview, restore, restore_projected, sha256_digest, sha256_hex,
    FilePreview, RecoveryOutcome, RenderedWriteOutcome, RenderedWriteRequest, RestoreOutcome,
    SwitchError, SwitchOutcome, SwitchRequest,
};
pub use extensions::{
    apply_extension_plan, journal_lock_targets, recover_pending, ExtensionApplyRequest,
    ExtensionError, LibraryCommitError, RecoveryReport,
};
pub use io::{FsIo, SwitchIo};
pub use lockfile::{
    acquire, lock_path_for, probe_lock, probe_lock_with, recover_stale, release, AcquireOutcome,
    RecoveryEntry,
};
pub use prompt_documents::{
    read_global_prompt_document, write_global_prompt_document, GlobalPromptDocumentOutcome,
    GlobalPromptDocumentRequest,
};
pub use restore::list_backups;
pub use subagent_settings::{
    preview_codex_subagent_settings, read_codex_subagent_settings, recovery_label,
    write_codex_subagent_settings, SubagentSettingsRequest,
};

pub use config_journal::{
    config_journal_path, finish_config_recovery, pending_config_write, rollback_pending_config,
    PendingConfigWrite,
};
