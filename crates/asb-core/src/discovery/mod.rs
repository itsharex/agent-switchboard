//! Read-only local discovery.
//!
//! This module never touches the filesystem: the caller injects a read
//! function and the paths to inspect. The report contains parsed facts and
//! warnings only — no raw file content — and anything secret-shaped is
//! redacted.

mod import;
mod inspect;
mod report;
#[cfg(test)]
mod tests;

pub use import::claude_import_proposal;
pub use inspect::inspect;
pub use report::{
    ClaudeImportProposal, DiscoveredFile, DiscoveredState, DiscoveryPaths, DiscoveryReport,
};

use crate::contracts::AppKind;
use crate::discovery::inspect::inspect_read;

/// Runs discovery over injected reads. `read` returns content, a missing-file
/// result, or a safe read error. Nothing is ever written.
pub fn discover(
    paths: &DiscoveryPaths,
    read: impl Fn(&str) -> Result<Option<String>, String>,
) -> DiscoveryReport {
    let codex_text = read(&paths.codex);
    let claude_text = read(&paths.claude);
    let codex = inspect_read(AppKind::Codex, &paths.codex, codex_text.clone());
    let claude = inspect_read(AppKind::Claude, &paths.claude, claude_text.clone());
    let claude_import_proposals = [claude_import_proposal(
        &claude,
        claude_text.ok().flatten().as_deref(),
    )]
    .into_iter()
    .flatten()
    .collect();
    DiscoveryReport {
        codex,
        claude,
        claude_import_proposals,
    }
}
