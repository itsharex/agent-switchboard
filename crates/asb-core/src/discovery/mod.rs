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

pub use import::import_proposal;
pub use inspect::inspect;
pub use report::{DiscoveredFile, DiscoveredState, DiscoveryPaths, DiscoveryReport};

use crate::contracts::{AppKind, RouteMode};
use crate::discovery::import::codex_auth_api_key;
use crate::discovery::inspect::inspect_read;

/// Runs discovery over injected reads. `read` returns content, a missing-file
/// result, or a safe read error. Nothing is ever written.
pub fn discover(
    paths: &DiscoveryPaths,
    read: impl Fn(&str) -> Result<Option<String>, String>,
) -> DiscoveryReport {
    let codex_text = read(&paths.codex);
    let codex_auth = read(&paths.codex_auth);
    let claude_text = read(&paths.claude);
    let mut codex = inspect_read(AppKind::Codex, &paths.codex, codex_text.clone());
    if let DiscoveredState::Ok {
        route,
        warnings,
        importable,
        ..
    } = &mut codex.state
    {
        if route.route_mode == RouteMode::Custom
            && codex_auth_api_key(codex_auth.as_ref().ok().and_then(|text| text.as_deref()))
                .is_none()
        {
            *importable = false;
            warnings.push("当前 Codex API-key 登录缓存不可导入".to_string());
        }
    }
    let claude = inspect_read(AppKind::Claude, &paths.claude, claude_text.clone());
    let import_proposals = [
        (import_proposal(
            &codex,
            codex_text.ok().flatten().as_deref(),
            codex_auth.ok().flatten().as_deref(),
        )),
        (import_proposal(&claude, claude_text.ok().flatten().as_deref(), None)),
    ]
    .into_iter()
    .flatten()
    .collect();
    DiscoveryReport {
        codex,
        claude,
        import_proposals,
    }
}
