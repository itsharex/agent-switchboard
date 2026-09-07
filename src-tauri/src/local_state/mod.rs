//! Application-owned local state: desktop preferences, optional cloud-backup
//! connection coordinates, and the Codex reset-signal cache. Provider,
//! common-settings, and write-history storage live in [`crate::config_store`];
//! this struct only hands out its store and resolves the real client paths,
//! which are never created here.

mod caches;
mod paths;
mod settings;

#[cfg(test)]
mod tests;

pub(crate) use paths::user_home_dir;
pub use settings::{AppSettings, CloseBehavior, CloudBackupSettings};

use crate::config_store::ConfigStore;
use asb_core::contracts::AppKind;
use paths::{
    claude_credentials_path_in_home, codex_auth_path_in_home, global_prompt_target_in_home,
    target_in_home,
};
use std::path::{Path, PathBuf};

pub struct LocalState {
    root: PathBuf,
}

impl LocalState {
    pub fn from_app(app: &tauri::AppHandle) -> Result<Self, String> {
        use tauri::Manager;

        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|_| "无法定位应用数据目录".to_string())?;
        let state = Self::from_app_data_dir(app_data_dir);
        state.ensure_extension_schema()?;
        Ok(state)
    }

    /// Resolves the same app-data location Tauri uses before an AppHandle
    /// exists, so startup-only WebView preferences can be read in time.
    pub(crate) fn from_startup_identifier(identifier: &str) -> Result<Self, String> {
        let app_data_dir = dirs::data_dir()
            .ok_or_else(|| "无法定位应用数据目录".to_string())?
            .join(identifier);
        let state = Self::from_app_data_dir(app_data_dir);
        state.ensure_extension_schema()?;
        Ok(state)
    }

    /// Brings the persisted extension library to the current schema once at
    /// startup: an interrupted swap is recovered, then the one-shot offline
    /// migration runs. Failure blocks startup instead of risking a second
    /// writer against a rejected schema.
    fn ensure_extension_schema(&self) -> Result<(), String> {
        let root = self.root.join("extensions");
        crate::extensions::migrate::recover_interrupted_migration(&root)?;
        crate::extensions::migrate::migrate_library_if_needed(&root).map(|_| ())
    }

    fn from_app_data_dir(app_data_dir: PathBuf) -> Self {
        Self {
            root: app_data_dir.join("state"),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_root(root: PathBuf) -> Self {
        Self { root }
    }

    /// The application configuration store rooted at this state directory.
    /// Every provider, common-settings, and history operation goes through
    /// it; this struct keeps no second copy of that data.
    pub fn configuration(&self) -> ConfigStore {
        ConfigStore::new(self.root.clone())
    }

    /// The `state/` directory itself; extension storage lives beside the
    /// provider store under `state/extensions`.
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    pub fn target(&self, app: AppKind) -> Result<PathBuf, String> {
        Self::user_config_path(app)
    }

    /// Resolves the one global instruction document owned by each supported
    /// client. Both clients honor their explicit configuration directories.
    pub fn global_prompt_target(&self, app: AppKind) -> Result<PathBuf, String> {
        Self::global_prompt_path(app)
    }

    pub fn user_config_path(app: AppKind) -> Result<PathBuf, String> {
        let home = user_home_dir()?;
        let codex_home = std::env::var_os("CODEX_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let claude_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        Ok(target_in_home(
            &home,
            codex_home.as_deref(),
            claude_dir.as_deref(),
            app,
        ))
    }

    pub fn global_prompt_path(app: AppKind) -> Result<PathBuf, String> {
        let home = user_home_dir()?;
        let codex_home = std::env::var_os("CODEX_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let claude_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        Ok(global_prompt_target_in_home(
            Path::new(&home),
            codex_home.as_deref(),
            claude_dir.as_deref(),
            app,
        ))
    }

    /// Resolves the existing Codex login cache without reading, creating, or
    /// modifying it. Codex honors an explicit CODEX_HOME for this user-owned
    /// state just as it does for its global instruction document.
    pub fn codex_auth_path() -> Result<PathBuf, String> {
        let home = user_home_dir()?;
        let codex_home = std::env::var_os("CODEX_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        Ok(codex_auth_path_in_home(
            Path::new(&home),
            codex_home.as_deref(),
        ))
    }

    /// Resolves the Claude login cache without reading, creating, or modifying
    /// it. Claude honors an explicit CLAUDE_CONFIG_DIR for this user-owned
    /// state, mirroring how the Codex resolver honors CODEX_HOME.
    pub fn claude_credentials_path() -> Result<PathBuf, String> {
        let home = user_home_dir()?;
        let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        Ok(claude_credentials_path_in_home(
            Path::new(&home),
            config_dir.as_deref(),
        ))
    }

    pub fn backup_dir(&self) -> PathBuf {
        self.root.join("backups")
    }

    /// Private state for the loopback protocol gateway. It contains its
    /// listener identity and the active profile fingerprints, never a copied
    /// upstream API key or a client configuration document.
    pub(crate) fn gateway_state_path(&self) -> PathBuf {
        self.root.join("gateway.json")
    }

    /// Prompt-document backups stay in their own collection so configuration
    /// restore and undo never offer a document snapshot as a client config.
    pub fn prompt_backup_dir(&self) -> PathBuf {
        self.backup_dir().join("prompts")
    }

    fn settings_path(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    fn codex_reset_cache_path(&self) -> PathBuf {
        self.root.join("codex-reset-cache.json")
    }

    fn codex_quota_baseline_path(&self) -> PathBuf {
        self.root.join("codex-quota-baseline.json")
    }

    fn usage_cache_path(&self) -> PathBuf {
        self.root.join("usage-cache.json")
    }

    fn model_usage_cache_path(&self) -> PathBuf {
        self.root.join("model-usage-cache.json")
    }

    /// The single credential-free usage-history ledger. Its schema, parsing,
    /// pruning, and atomic replacement are owned by `usage_history`.
    pub(crate) fn usage_history_path(&self) -> PathBuf {
        self.root.join("usage-history.json")
    }

    fn discovery_cache_path(&self) -> PathBuf {
        self.root.join("discovery-cache.json")
    }

    fn cloud_backup_settings_path(&self) -> PathBuf {
        self.root.join("cloud-backup.json")
    }
}
