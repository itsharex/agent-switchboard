//! Shared fixtures for the switch integration tests: plan builders, the
//! accepted-commit wrappers, isolated directory setup, the failure-injecting
//! IO wrapper, and the lock-path probe.
#![allow(dead_code)]

pub mod extensions;

use asb_core::contracts::{
    AppKind, ConfigValue, ProviderProfile, SettingValue, SwitchPlan, UpstreamProtocol,
};
use asb_core::ownership::{default_client_settings, default_provider_parameters};
use asb_core::test_support::CODEX_TOML;
use asb_switch::io::{FsIo, SwitchIo};
use asb_switch::lockfile;
use asb_switch::{RestoreOutcome, SwitchError};
use std::cell::Cell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn codex_plan(name: &str, base_url: &str, model: &str, cred: &str) -> SwitchPlan {
    let mut parameters = default_provider_parameters(AppKind::Codex);
    parameters.settings.insert(
        "model_reasoning_effort".into(),
        SettingValue::Explicit {
            value: ConfigValue::Str("xhigh".into()),
        },
    );
    SwitchPlan::through_gateway(
        ProviderProfile {
            authentication: None,
            connection: Default::default(),
            id: format!("id-{name}"),
            app: AppKind::Codex,
            route_mode: asb_core::RouteMode::Custom,
            name: name.into(),
            model: Some(model.into()),
            base_url: Some(base_url.into()),
            api_key: cred.into(),
            upstream_protocol: Some(UpstreamProtocol::Responses),
            responses_options: Some(asb_core::contracts::ResponsesOptions {
                request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
            }),
            max_output_tokens: None.into(),
            model_options: None,
            parameters,
            claude_fragment: Default::default(),
            notes: None,
            website_url: None,
            display: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        },
        default_client_settings(AppKind::Codex),
        format!(
            "http://127.0.0.1:18900/codex/asb_codex_{}/v1",
            "a".repeat(64)
        ),
        "".into(),
    )
}

pub fn claude_plan(name: &str, base_url: &str, model: &str) -> SwitchPlan {
    let mut parameters = default_provider_parameters(AppKind::Claude);
    parameters.settings.insert(
        "ultracode".into(),
        SettingValue::Explicit {
            value: ConfigValue::Bool(true),
        },
    );
    SwitchPlan::direct(
        ProviderProfile {
            authentication: None,
            connection: Default::default(),
            id: format!("id-{name}"),
            app: AppKind::Claude,
            route_mode: asb_core::RouteMode::Custom,
            name: name.into(),
            model: Some(model.into()),
            base_url: Some(base_url.into()),
            api_key: "test-api-key".into(),
            upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
            responses_options: None,
            max_output_tokens: None.into(),
            model_options: None,
            parameters,
            claude_fragment: Default::default(),
            notes: None,
            website_url: None,
            display: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        },
        default_client_settings(AppKind::Claude),
    )
}

/// Ordinary executor tests must still provide the explicit state commit
/// closure required by the production API. This helper supplies the accepted
/// state write so individual tests can focus on their own transaction path.
pub fn execute<Io: SwitchIo>(
    io: &Io,
    request: &asb_switch::SwitchRequest,
) -> Result<asb_switch::SwitchOutcome, SwitchError> {
    asb_switch::execute(io, request, |_| Ok(()))
}

/// Ordinary restore tests also provide the required state commit closure.
pub fn restore<Io: SwitchIo>(
    io: &Io,
    backup: &asb_core::BackupRecord,
    target: &Path,
) -> Result<RestoreOutcome, SwitchError> {
    asb_switch::restore(io, backup, target, |_| Ok(()))
}

pub fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

pub fn setup(app: AppKind, content: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let (file, initial) = match app {
        AppKind::Codex => ("config.toml", content),
        AppKind::Claude => ("settings.json", content),
    };
    let target = dir.path().join("live").join(file);
    write(&target, initial);
    let backup_dir = dir.path().join("backups");
    (dir, target, backup_dir)
}

/// Wraps FsIo with deterministic failure injection at a chosen stage.
pub struct FailingIo {
    pub fail_stage: Cell<Option<&'static str>>,
    pub fixed_now: Option<&'static str>,
    pub renamed: Cell<bool>,
    /// When set, the first read of the live file after a rename returns
    /// corrupted content, simulating a post-verify failure with the file
    /// already replaced. One-shot: the restore path must read cleanly.
    pub corrupt_once: Cell<bool>,
    pub corrupt_after_rename: bool,
    /// Corrupts only the executor's backup re-read response without changing
    /// the isolated temporary directory on disk.
    pub corrupt_backup_read: bool,
    /// Corrupts only the executor's temporary-file re-read response.
    pub corrupt_temp_read: bool,
    /// Simulates a host process editing the live Codex target while the
    /// executor is preparing its temporary candidate.
    pub mutate_live_after_temp_write: bool,
}

impl FailingIo {
    pub fn new() -> Self {
        Self {
            fail_stage: Cell::new(None),
            fixed_now: None,
            renamed: Cell::new(false),
            corrupt_once: Cell::new(false),
            corrupt_after_rename: false,
            corrupt_backup_read: false,
            corrupt_temp_read: false,
            mutate_live_after_temp_write: false,
        }
    }

    fn check(&self, stage: &'static str) -> io::Result<()> {
        if self.fail_stage.get() == Some(stage) {
            return Err(io::Error::other(format!("injected failure at {stage}")));
        }
        Ok(())
    }

    fn is_live_target(&self, path: &Path) -> bool {
        path.file_name()
            .map(|n| n == "config.toml")
            .unwrap_or(false)
    }
}

impl SwitchIo for FailingIo {
    fn sync_file(&self, path: &Path) -> io::Result<()> {
        FsIo.sync_file(path)
    }
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        FsIo.sync_dir(path)
    }
    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>> {
        FsIo.read_bytes(path)
    }
    fn write_new_bytes(&self, path: &Path, content: &[u8]) -> io::Result<()> {
        FsIo.write_new_bytes(path, content)
    }
    fn set_mode(&self, path: &Path, mode: u32) -> io::Result<()> {
        FsIo.set_mode(path, mode)
    }
    fn write_bytes_replace(&self, path: &Path, content: &[u8]) -> io::Result<()> {
        FsIo.write_bytes_replace(path, content)
    }
    fn path_kind(&self, path: &Path) -> io::Result<asb_switch::io::PathKind> {
        FsIo.path_kind(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        FsIo.rename(from, to)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        FsIo.remove_dir_all(path)
    }
    fn read_file(&self, path: &Path) -> io::Result<String> {
        let text = fs::read_to_string(path)?;
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if self.corrupt_backup_read && name.ends_with(".bak") {
            return Ok(format!("{text}\ncorrupted backup read"));
        }
        if self.corrupt_temp_read && (name.contains(".asb-tmp") || name.contains(".asb-restore")) {
            return Ok(format!("{text}\ncorrupted temporary read"));
        }
        if self.corrupt_after_rename
            && !self.corrupt_once.get()
            && self.renamed.get()
            && self.is_live_target(path)
        {
            self.corrupt_once.set(true);
            return Ok(text + "\n# corrupted");
        }
        Ok(text)
    }

    fn write_new_file(&self, path: &Path, content: &str) -> io::Result<()> {
        // Only the switch temp file may be forced to fail; lock acquisition
        // and backup creation must keep working.
        if path.to_string_lossy().ends_with(".asb-tmp") {
            self.check("temp-write")?;
        }
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(content.as_bytes())?;
        if self.mutate_live_after_temp_write && path.to_string_lossy().ends_with(".asb-tmp") {
            fs::write(
                path.parent().expect("temp parent").join("config.toml"),
                format!("{CODEX_TOML}\nhost_changed_during_switch = true\n"),
            )?;
        }
        Ok(())
    }

    fn write_file_replace(&self, path: &Path, content: &str) -> io::Result<()> {
        fs::write(path, content)
    }

    fn rename_replace(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.check("atomic-replace")?;
        fs::rename(from, to)?;
        self.renamed.set(true);
        Ok(())
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".asb-lock"))
        {
            self.check("lock-release")?;
        }
        fs::remove_file(path)
    }

    fn ensure_dir(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(path)? {
            out.push(entry?.path());
        }
        Ok(out)
    }

    fn now_rfc3339(&self) -> String {
        self.fixed_now
            .map(str::to_string)
            .unwrap_or_else(|| FsIo.now_rfc3339())
    }
}

pub fn lock_path_exists(target: &Path) -> bool {
    lockfile::lock_path_for(target).exists()
}
