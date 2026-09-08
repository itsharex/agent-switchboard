//! Transactional writes for the Codex `[agents]` default settings.
//!
//! The user-level `config.toml` is the only source of truth: nothing here
//! keeps a second copy. Reads and renders are pure; the single write reuses
//! the executor's rendered-write transaction — lock, external-change check,
//! backup, temporary write, syntax validation, atomic replacement, post-write
//! verification, and rollback from the immediately preceding backup.
//!
//! No supplier profile is involved and no supplier switch is triggered; the
//! transaction targets one file and three global runtime scalars.

use std::io::ErrorKind;
use std::path::Path;

use asb_core::adapter::codex::{
    deprecated_subagent_keys, read_subagent_settings, render_subagent_settings,
};
use asb_core::{
    AppKind, CodexSubagentSettings, CodexSubagentSettingsPreview, CodexSubagentSettingsSnapshot,
};

use crate::display::display_content;
use crate::executor::{
    execute_rendered, sha256_hex, RecoveryOutcome, RenderedWriteRequest, SwitchError,
};
use crate::io::SwitchIo;

/// The renderer supplies the validated settings and the hash it last read.
/// It never supplies a target path; command code resolves that backend-only
/// value.
pub struct SubagentSettingsRequest<'a> {
    pub target: &'a Path,
    pub backup_dir: &'a Path,
    pub settings: &'a CodexSubagentSettings,
    /// Hash of the file when the preview was produced.
    pub expected_hash: &'a str,
    pub expected_target_existed: bool,
    /// Hash of the exact candidate the user confirmed.
    pub expected_rendered_hash: &'a str,
}

fn adapter_error(error: asb_core::adapter::AdapterError) -> SwitchError {
    SwitchError::PlanRejected {
        message: error.message,
        line: error.line,
    }
}

fn read_or_empty<Io: SwitchIo>(io: &Io, target: &Path) -> Result<(String, bool), SwitchError> {
    match io.read_file(target) {
        Ok(content) => Ok((content, true)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok((String::new(), false)),
        Err(error) => Err(SwitchError::ReadCurrent {
            message: error.to_string(),
        }),
    }
}

fn snapshot_of(content: &str, exists: bool) -> Result<CodexSubagentSettingsSnapshot, SwitchError> {
    let settings = read_subagent_settings(content).map_err(adapter_error)?;
    Ok(CodexSubagentSettingsSnapshot {
        app: AppKind::Codex,
        settings,
        config_hash: sha256_hex(content),
        file_exists: exists,
        deprecated_keys: deprecated_subagent_keys(content).map_err(adapter_error)?,
    })
}

/// Reads the three global runtime scalars from the real user-level configuration.
/// A missing file reads as all-automatic without creating anything.
pub fn read_codex_subagent_settings<Io: SwitchIo>(
    io: &Io,
    target: &Path,
) -> Result<CodexSubagentSettingsSnapshot, SwitchError> {
    let (content, exists) = read_or_empty(io, target)?;
    snapshot_of(&content, exists)
}

/// Renders the complete candidate document without reading or writing any
/// file. The returned content is the redacted display copy; the real
/// candidate stays private and is identified by `rendered_hash`.
pub fn preview_codex_subagent_settings(
    current: &str,
    settings: &CodexSubagentSettings,
) -> Result<CodexSubagentSettingsPreview, SwitchError> {
    let rendered = render_subagent_settings(current, settings).map_err(adapter_error)?;
    Ok(CodexSubagentSettingsPreview {
        app: AppKind::Codex,
        target: AppKind::Codex.config_label().to_string(),
        content: display_content(AppKind::Codex, &rendered),
        config_hash: sha256_hex(current),
        rendered_hash: sha256_hex(&rendered),
    })
}

/// Applies one confirmed sub-agent settings plan. The file is re-read inside
/// the transaction lock, so an external edit after the preview is refused
/// rather than merged.
pub fn write_codex_subagent_settings<Io: SwitchIo>(
    io: &Io,
    request: &SubagentSettingsRequest,
) -> Result<CodexSubagentSettingsSnapshot, SwitchError> {
    let (current, existed) = read_or_empty(io, request.target)?;
    let found_hash = sha256_hex(&current);
    if found_hash != request.expected_hash || existed != request.expected_target_existed {
        return Err(SwitchError::ExternalChange {
            expected_hash: request.expected_hash.to_string(),
            found_hash,
        });
    }
    let rendered = render_subagent_settings(&current, request.settings).map_err(adapter_error)?;
    if sha256_hex(&rendered) != request.expected_rendered_hash {
        return Err(SwitchError::PlanChanged);
    }
    execute_rendered(
        io,
        &RenderedWriteRequest {
            target: request.target,
            app: AppKind::Codex,
            backup_dir: request.backup_dir,
            expected_hash: request.expected_hash,
            expected_target_existed: request.expected_target_existed,
            rendered: &rendered,
            reason: "subagent-settings",
        },
        |_| Ok(()),
    )?;
    let (content, exists) = read_or_empty(io, request.target)?;
    snapshot_of(&content, exists)
}

/// Maps a write failure onto the user-visible recovery wording used by the
/// sub-agent module.
pub fn recovery_label(recovery: &RecoveryOutcome) -> &'static str {
    match recovery {
        RecoveryOutcome::NotNeeded => "真实配置未被替换",
        RecoveryOutcome::Restored { .. } => "已恢复写入前的内容",
        RecoveryOutcome::RestoreFailed { .. } => "无法自动恢复，请从应用备份恢复",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asb_core::contracts::{ConfigValue, SettingValue};
    use std::fs;

    fn temp_target(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join(name);
        (directory, target)
    }

    fn explicit_bool(value: bool) -> SettingValue {
        SettingValue::Explicit {
            value: ConfigValue::Bool(value),
        }
    }

    fn settings_with_enabled() -> CodexSubagentSettings {
        CodexSubagentSettings {
            enabled: explicit_bool(true),
            ..CodexSubagentSettings::automatic()
        }
    }

    #[test]
    fn a_missing_file_reads_as_all_automatic_without_creating_it() {
        let (_guard, target) = temp_target("config.toml");
        let snapshot =
            read_codex_subagent_settings(&crate::io::FsIo, &target).expect("read missing file");
        assert_eq!(snapshot.settings, CodexSubagentSettings::automatic());
        assert!(!snapshot.file_exists);
        assert!(!target.exists());
    }

    #[test]
    fn reading_reports_canonical_scalars_and_deprecated_keys() {
        let (_guard, target) = temp_target("config.toml");
        fs::write(&target, "[agents]\nmax_threads = 4\nenabled = true\n").unwrap();
        let snapshot = read_codex_subagent_settings(&crate::io::FsIo, &target).expect("read");
        assert_eq!(
            snapshot.settings.enabled,
            SettingValue::Explicit {
                value: ConfigValue::Bool(true)
            }
        );
        assert_eq!(
            snapshot.deprecated_keys,
            vec!["agents.max_threads".to_string()]
        );
    }

    #[test]
    fn preview_is_pure_and_redacts_the_candidate() {
        let current =
            "model = \"gpt-5\"\nexperimental_bearer_token = \"sk-live-0123456789abcdef\"\n";
        let first =
            preview_codex_subagent_settings(current, &settings_with_enabled()).expect("preview");
        let second =
            preview_codex_subagent_settings(current, &settings_with_enabled()).expect("preview");
        assert_eq!(first, second);
        assert!(!first.content.contains("sk-live-0123456789abcdef"));
        assert!(first.content.contains("enabled = true"));
        assert_eq!(first.config_hash, sha256_hex(current));
    }

    #[test]
    fn a_stale_baseline_hash_is_refused_before_any_write() {
        let (guard, target) = temp_target("config.toml");
        fs::write(&target, "model = \"gpt-5\"\n").unwrap();
        let error = write_codex_subagent_settings(
            &crate::io::FsIo,
            &SubagentSettingsRequest {
                target: &target,
                backup_dir: &guard.path().join("backups"),
                settings: &settings_with_enabled(),
                expected_hash: "stale",
                expected_target_existed: true,
                expected_rendered_hash: "stale",
            },
        )
        .expect_err("stale hash must fail");
        assert!(matches!(error, SwitchError::ExternalChange { .. }));
        assert_eq!(fs::read_to_string(&target).unwrap(), "model = \"gpt-5\"\n");
    }

    #[test]
    fn a_stale_rendered_hash_is_refused() {
        let (guard, target) = temp_target("config.toml");
        fs::write(&target, "model = \"gpt-5\"\n").unwrap();
        let current = fs::read_to_string(&target).unwrap();
        let error = write_codex_subagent_settings(
            &crate::io::FsIo,
            &SubagentSettingsRequest {
                target: &target,
                backup_dir: &guard.path().join("backups"),
                settings: &settings_with_enabled(),
                expected_hash: &sha256_hex(&current),
                expected_target_existed: true,
                expected_rendered_hash: "not-the-previewed-candidate",
            },
        )
        .expect_err("stale candidate must fail");
        assert!(matches!(error, SwitchError::PlanChanged));
        assert_eq!(fs::read_to_string(&target).unwrap(), current);
    }

    #[test]
    fn applying_writes_atomically_and_reports_a_fresh_snapshot() {
        let (guard, target) = temp_target("config.toml");
        let current = "model = \"gpt-5\"\n[agents.worker]\ndescription = \"worker\"\n";
        fs::write(&target, current).unwrap();
        let preview =
            preview_codex_subagent_settings(current, &settings_with_enabled()).expect("preview");
        let snapshot = write_codex_subagent_settings(
            &crate::io::FsIo,
            &SubagentSettingsRequest {
                target: &target,
                backup_dir: &guard.path().join("backups"),
                settings: &settings_with_enabled(),
                expected_hash: &preview.config_hash,
                expected_target_existed: true,
                expected_rendered_hash: &preview.rendered_hash,
            },
        )
        .expect("apply");
        let written = fs::read_to_string(&target).unwrap();
        assert!(written.contains("model = \"gpt-5\""));
        assert!(written.contains("[agents.worker]"));
        assert!(written.contains("description = \"worker\""));
        assert!(written.contains("enabled = true"));
        assert_eq!(snapshot.config_hash, sha256_hex(&written));
        assert_eq!(snapshot.settings.enabled, explicit_bool(true));
        let backups = fs::read_dir(guard.path().join("backups"))
            .expect("backup directory")
            .count();
        assert!(backups >= 2, "backup file and its metadata must exist");
    }

    #[test]
    fn a_concurrent_external_edit_is_preserved_not_overwritten() {
        let (guard, target) = temp_target("config.toml");
        let current = "model = \"gpt-5\"\n";
        fs::write(&target, current).unwrap();
        let preview =
            preview_codex_subagent_settings(current, &settings_with_enabled()).expect("preview");
        // Simulate an edit that lands after the preview but before apply.
        fs::write(&target, "model = \"externally-changed\"\n").unwrap();
        let error = write_codex_subagent_settings(
            &crate::io::FsIo,
            &SubagentSettingsRequest {
                target: &target,
                backup_dir: &guard.path().join("backups"),
                settings: &settings_with_enabled(),
                expected_hash: &preview.config_hash,
                expected_target_existed: true,
                expected_rendered_hash: &preview.rendered_hash,
            },
        )
        .expect_err("external edit must be refused");
        assert!(matches!(error, SwitchError::ExternalChange { .. }));
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "model = \"externally-changed\"\n"
        );
    }

    #[test]
    fn a_deprecated_concurrency_key_blocks_the_write() {
        let (guard, target) = temp_target("config.toml");
        let current = "[agents]\nmax_threads = 4\n";
        fs::write(&target, current).unwrap();
        let settings = CodexSubagentSettings {
            max_concurrent_threads_per_session: SettingValue::Explicit {
                value: ConfigValue::Number(3.0),
            },
            ..CodexSubagentSettings::automatic()
        };
        let error = write_codex_subagent_settings(
            &crate::io::FsIo,
            &SubagentSettingsRequest {
                target: &target,
                backup_dir: &guard.path().join("backups"),
                settings: &settings,
                expected_hash: &sha256_hex(current),
                expected_target_existed: true,
                expected_rendered_hash: &sha256_hex(
                    &render_subagent_settings(current, &settings).unwrap_or_default(),
                ),
            },
        )
        .expect_err("deprecated key conflicts");
        assert!(matches!(error, SwitchError::PlanRejected { .. }));
        assert_eq!(fs::read_to_string(&target).unwrap(), current);
    }
}
