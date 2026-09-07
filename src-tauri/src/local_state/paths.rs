use asb_core::contracts::AppKind;
use std::path::{Path, PathBuf};

/// Resolves the user home directory across supported platforms: `USERPROFILE`
/// on Windows, `HOME` elsewhere. The first non-empty value wins.
pub(crate) fn user_home_dir() -> Result<PathBuf, String> {
    for key in ["USERPROFILE", "HOME"] {
        if let Some(home) = std::env::var_os(key).filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(home));
        }
    }
    Err("无法确定用户主目录".to_string())
}

pub(super) fn target_in_home(
    home: &Path,
    codex_home: Option<&Path>,
    claude_dir: Option<&Path>,
    app: AppKind,
) -> PathBuf {
    match app {
        AppKind::Codex => match codex_home {
            Some(directory) => directory.join("config.toml"),
            None => home.join(".codex").join("config.toml"),
        },
        AppKind::Claude => {
            claude_credentials_path_in_home(home, claude_dir).with_file_name("settings.json")
        }
    }
}

pub(super) fn global_prompt_target_in_home(
    home: &Path,
    codex_home: Option<&Path>,
    claude_dir: Option<&Path>,
    app: AppKind,
) -> PathBuf {
    target_in_home(home, codex_home, claude_dir, app).with_file_name(app.global_prompt_file_name())
}

pub(super) fn codex_auth_path_in_home(home: &Path, codex_home: Option<&Path>) -> PathBuf {
    match codex_home {
        Some(directory) => directory.join("auth.json"),
        None => home.join(".codex").join("auth.json"),
    }
}

pub(super) fn claude_credentials_path_in_home(home: &Path, config_dir: Option<&Path>) -> PathBuf {
    match config_dir {
        Some(directory) => directory.join(".credentials.json"),
        None => home.join(".claude").join(".credentials.json"),
    }
}
