//! Codex native paths share one root; resolving them never creates files.

use std::path::{Path, PathBuf};

pub(crate) fn root_in_home(home: &Path, override_root: Option<&Path>) -> PathBuf {
    override_root
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".codex"))
}

pub(crate) fn session_roots(root: &Path) -> [PathBuf; 2] {
    [root.join("sessions"), root.join("archived_sessions")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_roots_never_include_the_default_home() {
        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path().join("home");
        let first = temporary.path().join("first");
        let second = temporary.path().join("second");
        for root in [&first, &second] {
            let resolved = root_in_home(&home, Some(root));
            assert_eq!(resolved, *root);
            assert_eq!(
                session_roots(&resolved),
                [root.join("sessions"), root.join("archived_sessions")]
            );
            assert!(!resolved.exists(), "path resolution is read-only");
            assert!(!resolved.starts_with(&home));
        }
    }

    #[test]
    fn empty_overrides_use_the_native_default() {
        let home = Path::new("fixture-home");
        assert_eq!(root_in_home(home, None), home.join(".codex"));
        assert_eq!(root_in_home(home, Some(Path::new(""))), home.join(".codex"));
    }
}
