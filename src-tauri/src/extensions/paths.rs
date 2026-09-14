//! Path resolution for the extensions workspace.
//!
//! All functions are pure path composition over injected environment facts
//! (`home`, `CODEX_HOME`, `CLAUDE_CONFIG_DIR`, a project root) so discovery
//! and planning can be tested against fixture directories without touching
//! the user's real environment. They never create directories.

use std::path::{Path, PathBuf};

use asb_core::contracts::AppKind;

/// The Codex configuration document: `$CODEX_HOME/config.toml` or
/// `~/.codex/config.toml`.
pub fn codex_config_path(home: &Path, codex_home: Option<&Path>) -> PathBuf {
    crate::local_state::codex_paths::root_in_home(home, codex_home).join("config.toml")
}

/// The Claude user document holding `mcpServers` and `projects`: the
/// documented default is `~/.claude.json`.
pub fn claude_user_json_path(home: &Path, claude_dir: Option<&Path>) -> PathBuf {
    match claude_dir {
        Some(directory) => directory.join(".claude.json"),
        None => home.join(".claude.json"),
    }
}

/// The Claude settings document (skill visibility overrides and other
/// settings): `<config-dir>/settings.json`.
pub fn claude_settings_path(home: &Path, claude_dir: Option<&Path>) -> PathBuf {
    let fallback;
    let directory: &Path = match claude_dir {
        Some(directory) => directory,
        None => {
            fallback = home.join(".claude");
            &fallback
        }
    };
    directory.join("settings.json")
}

/// Claude's per-user project-local settings exception.
pub fn claude_project_local_settings_path(project_root: &Path) -> PathBuf {
    project_root.join(".claude").join("settings.local.json")
}

/// Claude's shared project settings.
pub fn claude_project_settings_path(project_root: &Path) -> PathBuf {
    project_root.join(".claude").join("settings.json")
}

/// Codex's current user skill root per the official documentation:
/// `~/.agents/skills`. Other tools may also read this shared directory.
pub fn codex_user_skills_root(home: &Path) -> PathBuf {
    home.join(".agents").join("skills")
}

/// Project skill root for Codex: `<project>/.agents/skills`.
pub fn codex_project_skills_root(project_root: &Path) -> PathBuf {
    project_root.join(".agents").join("skills")
}

/// Claude's user skill root: `<config-dir>/skills`.
pub fn claude_user_skills_root(home: &Path, claude_dir: Option<&Path>) -> PathBuf {
    let fallback;
    let directory: &Path = match claude_dir {
        Some(directory) => directory,
        None => {
            fallback = home.join(".claude");
            &fallback
        }
    };
    directory.join("skills")
}

/// Project skill root for Claude: `<project>/.claude/skills`.
pub fn claude_project_skills_root(project_root: &Path) -> PathBuf {
    project_root.join(".claude").join("skills")
}

/// Historical Codex skill locations (`$CODEX_HOME/skills`). Discovery may
/// report what it finds there read-only; nothing is migrated or written.
pub fn codex_legacy_skills_roots(home: &Path, codex_home: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(directory) = codex_home {
        roots.push(directory.join("skills"));
    }
    roots.push(home.join(".codex").join("skills"));
    roots
}

/// The project-shared Claude MCP document.
pub fn claude_project_mcp_path(project_root: &Path) -> PathBuf {
    project_root.join(".mcp.json")
}

/// Codex's project configuration document (project-shared MCP).
pub fn codex_project_config_path(project_root: &Path) -> PathBuf {
    project_root.join(".codex").join("config.toml")
}

/// The user-level MCP document for one client.
pub fn user_mcp_document(
    client: AppKind,
    home: &Path,
    codex_home: Option<&Path>,
    claude_dir: Option<&Path>,
) -> PathBuf {
    match client {
        AppKind::Codex => codex_config_path(home, codex_home),
        AppKind::Claude => claude_user_json_path(home, claude_dir),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_paths_prefer_codex_home() {
        let home = Path::new("/home/u");
        let codex_home = Path::new("/custom/codex");
        assert_eq!(
            codex_config_path(home, Some(codex_home)),
            PathBuf::from("/custom/codex/config.toml")
        );
        assert_eq!(
            codex_config_path(home, None),
            PathBuf::from("/home/u/.codex/config.toml")
        );
        // The user skill root is never CODEX_HOME/skills.
        assert_eq!(
            codex_user_skills_root(home),
            PathBuf::from("/home/u/.agents/skills")
        );
    }

    #[test]
    fn claude_paths_move_with_the_config_dir() {
        let home = Path::new("/home/u");
        let claude_dir = Path::new("/custom/claude");
        assert_eq!(
            claude_user_skills_root(home, Some(claude_dir)),
            PathBuf::from("/custom/claude/skills")
        );
        assert_eq!(
            claude_settings_path(home, Some(claude_dir)),
            PathBuf::from("/custom/claude/settings.json")
        );
        assert_eq!(
            claude_user_skills_root(home, None),
            PathBuf::from("/home/u/.claude/skills")
        );
    }

    #[test]
    fn project_roots_are_composed_directly() {
        let root = Path::new("/work/repo");
        assert_eq!(
            claude_project_mcp_path(root),
            PathBuf::from("/work/repo/.mcp.json")
        );
        assert_eq!(
            codex_project_config_path(root),
            PathBuf::from("/work/repo/.codex/config.toml")
        );
        assert_eq!(
            claude_project_local_settings_path(root),
            PathBuf::from("/work/repo/.claude/settings.local.json")
        );
    }
}
