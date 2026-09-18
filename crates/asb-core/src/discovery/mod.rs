//! Read-only local discovery.
//!
//! This module never touches the filesystem: the caller injects a read
//! function and the paths to inspect. The report contains parsed facts and
//! warnings only — no raw file content — and anything secret-shaped is
//! redacted.

mod codex;
mod import;
mod inspect;
mod report;

pub use codex::{import_source as codex_import_source, proposal as codex_import_proposal};
pub use import::claude_import_proposal;
pub use inspect::inspect;
pub use report::{
    ClaudeImportProposal, CodexImportAction, CodexImportProposal, CodexImportSource,
    DiscoveredFile, DiscoveredState, DiscoveryPaths, DiscoveryReport,
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
    let codex_auth_text = read(&paths.codex_auth);
    let claude_text = read(&paths.claude);
    let mut codex = inspect_read(AppKind::Codex, &paths.codex, codex_text.clone());
    let claude = inspect_read(AppKind::Claude, &paths.claude, claude_text.clone());
    let codex_import_proposals = match codex_text.as_ref().ok().and_then(Option::as_ref) {
        Some(text) => match codex_import_proposal(
            &paths.codex,
            text,
            codex_auth_text.as_ref().ok().and_then(Option::as_deref),
            |path| read(&path.to_string_lossy()),
        ) {
            Ok(source) => source.map(|source| source.proposal).into_iter().collect(),
            Err(error) => {
                if let DiscoveredState::Ok {
                    managed,
                    warnings,
                    importable,
                    ..
                } = &mut codex.state
                {
                    if !*managed && !warnings.iter().any(|warning| warning == &error) {
                        warnings.push(error);
                    }
                    *importable = false;
                }
                Vec::new()
            }
        },
        None => Vec::new(),
    };
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
        codex_import_proposals,
        claude_import_proposals,
    }
}
