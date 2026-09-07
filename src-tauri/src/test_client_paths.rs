//! Test-only redirection of the real client configuration directories.
//!
//! `LocalState::target` / `codex_auth_path` resolve the user's real Codex and
//! Claude Code files from `CODEX_HOME` / `CLAUDE_CONFIG_DIR`. Every unit test
//! whose code path resolves or writes a client file must hold this guard for
//! its whole body, so no test in this process can touch real configuration.
//! The subprocess sandbox tests set up their own environment and never use
//! this module.

use std::sync::{Mutex, MutexGuard, OnceLock};
use tempfile::TempDir;

static CLIENT_PATH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) struct ClientPathGuard {
    _guard: MutexGuard<'static, ()>,
    directory: TempDir,
    previous_claude: Option<std::ffi::OsString>,
    previous_codex: Option<std::ffi::OsString>,
}

/// Redirects both client configuration roots to one fresh temporary
/// directory until the returned guard is dropped. Callers must hold the
/// guard across every operation that can resolve a client path.
pub(crate) fn redirect_client_paths() -> ClientPathGuard {
    let guard = CLIENT_PATH_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let directory = tempfile::tempdir().expect("temporary client-config dir");
    let previous_claude = std::env::var_os("CLAUDE_CONFIG_DIR");
    let previous_codex = std::env::var_os("CODEX_HOME");
    std::env::set_var("CLAUDE_CONFIG_DIR", directory.path());
    std::env::set_var("CODEX_HOME", directory.path());
    ClientPathGuard {
        _guard: guard,
        directory,
        previous_claude,
        previous_codex,
    }
}

impl Drop for ClientPathGuard {
    fn drop(&mut self) {
        match self.previous_claude.take() {
            Some(value) => std::env::set_var("CLAUDE_CONFIG_DIR", value),
            None => std::env::remove_var("CLAUDE_CONFIG_DIR"),
        }
        match self.previous_codex.take() {
            Some(value) => std::env::set_var("CODEX_HOME", value),
            None => std::env::remove_var("CODEX_HOME"),
        }
        // `directory` removes the temporary tree after the environment is
        // restored; `guard` is released when this destructor returns.
        drop(std::mem::replace(
            &mut self.directory,
            tempfile::tempdir().expect("temporary client-config dir on drop"),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn guard_redirects_and_restores_the_client_roots() {
        let resolved_before = crate::local_state::LocalState::from_root(PathBuf::from(
            std::env::temp_dir().join("asb-guard-check"),
        ))
        .target(asb_core::contracts::AppKind::Claude)
        .expect("claude target");

        let guard = redirect_client_paths();
        let local = crate::local_state::LocalState::from_root(PathBuf::from(
            std::env::temp_dir().join("asb-guard-check"),
        ));
        let redirected = local.target(asb_core::contracts::AppKind::Claude).unwrap();
        assert!(redirected.starts_with(guard.directory.path()));
        assert_ne!(redirected, resolved_before);
        drop(guard);

        let restored = local.target(asb_core::contracts::AppKind::Claude).unwrap();
        assert_eq!(restored, resolved_before);
    }
}
