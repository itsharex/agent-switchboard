use super::settings::{MotionPreference, ThemePreference};
use super::*;
use crate::codex_official_quota::{BaselineRead, CodexQuotaBaseline};
use crate::codex_reset::{CodexResetFeedStatus, CodexResetStatus, ResetSignal, ResetType};
use crate::runtime_log::RuntimeLogLevel;
use asb_core::contracts::{
    CodexOfficialQuotaReset, CodexOfficialQuotaResetKind, CodexOfficialQuotaWindow,
};
use std::fs;

fn cached_reset_status() -> CodexResetStatus {
    CodexResetStatus {
        source_url: "https://www.codexrunway.com/api/status.json".to_string(),
        feed_status: CodexResetFeedStatus::Ok,
        generated_at: "2026-08-31T03:08:02.232Z".to_string(),
        last_successful_check_at: "2026-08-31T03:08:02.232Z".to_string(),
        checked_at: "2026-08-31T03:10:00.000Z".to_string(),
        latest_confirmed_signal: Some(ResetSignal {
            announced_at: "2026-08-31T02:34:27Z".to_string(),
            effective_at: None,
            schedule_precision: None,
            confidence: 0.98,
            reset_type: ResetType::Global,
        }),
        next_scheduled_reset: None,
        latest_relevant_tibo_post: None,
        source_warning: None,
    }
}

fn stored_baseline() -> CodexQuotaBaseline {
    CodexQuotaBaseline {
        account_marker: Some("account-marker".to_string()),
        last_read: Some(BaselineRead {
            at: "2026-09-01T08:00:00Z".to_string(),
            windows: vec![CodexOfficialQuotaWindow {
                label: "7 天".to_string(),
                used_percent: 42.5,
                resets_at: Some("2026-09-04T00:00:00Z".to_string()),
            }],
        }),
        last_reset: Some(CodexOfficialQuotaReset {
            observed_at: "2026-08-31T02:34:27Z".to_string(),
            kind: CodexOfficialQuotaResetKind::Scheduled,
            resets_at: Some("2026-09-04T00:00:00Z".to_string()),
        }),
    }
}

#[test]
fn new_state_is_empty_and_does_not_create_configuration_files() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));

    assert!(!state.configuration().legacy_store_path().exists());
    assert!(!state.configuration().configuration_dir().exists());
    let target = target_in_home(&directory.path().join("home"), None, None, AppKind::Codex);
    assert!(!target.exists());
}

#[test]
fn global_prompt_targets_use_the_supported_client_document_names() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let home = directory.path().join("home");
    let codex_home = directory.path().join("custom-codex-home");

    assert_eq!(
        global_prompt_target_in_home(&home, None, None, AppKind::Codex),
        home.join(".codex").join("AGENTS.md")
    );
    assert_eq!(
        global_prompt_target_in_home(&home, Some(&codex_home), None, AppKind::Codex),
        codex_home.join("AGENTS.md")
    );
    assert_eq!(
        global_prompt_target_in_home(&home, None, None, AppKind::Claude),
        home.join(".claude").join("CLAUDE.md")
    );
}

#[test]
fn codex_auth_target_uses_the_same_home_resolution_without_creating_it() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let home = directory.path().join("home");
    let codex_home = directory.path().join("alternate-codex-home");

    assert_eq!(
        codex_auth_path_in_home(&home, None),
        home.join(".codex").join("auth.json")
    );
    assert_eq!(
        codex_auth_path_in_home(&home, Some(&codex_home)),
        codex_home.join("auth.json")
    );
    assert!(!home.exists());
    assert!(!codex_home.exists());
}

#[test]
fn codex_config_target_follows_codex_home() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let home = directory.path().join("home");
    let codex_home = directory.path().join("alternate-codex-home");

    assert_eq!(
        target_in_home(&home, Some(&codex_home), None, AppKind::Codex),
        codex_home.join("config.toml")
    );
}

#[test]
fn claude_credentials_target_follows_the_config_dir_without_creating_it() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let home = directory.path().join("home");
    let config_dir = directory.path().join("alternate-claude-config");

    assert_eq!(
        claude_credentials_path_in_home(&home, None),
        home.join(".claude").join(".credentials.json")
    );
    assert_eq!(
        claude_credentials_path_in_home(&home, Some(&config_dir)),
        config_dir.join(".credentials.json")
    );
    assert!(!home.exists());
    assert!(!config_dir.exists());
}

#[test]
fn claude_switch_and_prompt_targets_follow_the_credential_directory() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let custom = directory.path().join("alternate-claude-config");
    for config_dir in [None, Some(custom.as_path())] {
        let parent = claude_credentials_path_in_home(&home, config_dir)
            .parent()
            .unwrap()
            .to_path_buf();
        assert_eq!(
            target_in_home(&home, None, config_dir, AppKind::Claude),
            parent.join("settings.json")
        );
        assert_eq!(
            global_prompt_target_in_home(&home, None, config_dir, AppKind::Claude),
            parent.join("CLAUDE.md")
        );
    }
    assert!(!home.exists());
    assert!(!custom.exists());
}

#[test]
fn codex_reset_cache_is_absent_without_creating_a_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));

    assert_eq!(state.load_codex_reset_cache().expect("empty cache"), None);
    assert!(!state.codex_reset_cache_path().exists());
}

#[test]
fn cloud_backup_settings_are_optional_and_persist_without_passwords() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    let settings = CloudBackupSettings {
        project_url: "https://example.supabase.co".to_string(),
        publishable_key: "sb_publishable_example".to_string(),
        email: "backup@example.com".to_string(),
    };

    assert_eq!(state.get_cloud_backup_settings().expect("absent"), None);
    state
        .set_cloud_backup_settings(&settings)
        .expect("save settings");

    assert_eq!(
        state.get_cloud_backup_settings().expect("reload settings"),
        Some(settings)
    );
    let stored = fs::read_to_string(state.cloud_backup_settings_path()).expect("stored text");
    assert!(!stored.contains("password"));
}

#[test]
fn cloud_backup_settings_reject_a_non_https_or_noncanonical_project_url() {
    let invalid = CloudBackupSettings {
        project_url: "https://example.supabase.co/".to_string(),
        publishable_key: "sb_publishable_example".to_string(),
        email: "backup@example.com".to_string(),
    };

    assert_eq!(
        invalid.validate().expect_err("trailing slash"),
        "Supabase 项目地址必须是无尾随斜杠的 https URL"
    );
}

#[test]
fn codex_reset_cache_replaces_and_persists_the_latest_successful_snapshot() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().join("state");
    let state = LocalState::from_root(root.clone());
    let first = cached_reset_status();
    let latest = CodexResetStatus {
        checked_at: "2026-08-31T04:10:00.000Z".to_string(),
        next_scheduled_reset: Some(ResetSignal {
            announced_at: "2026-08-31T04:00:00Z".to_string(),
            effective_at: Some("2026-08-31T09:00:00Z".to_string()),
            schedule_precision: Some("datetime".to_string()),
            confidence: 0.84,
            reset_type: ResetType::Global,
        }),
        ..first.clone()
    };

    state
        .save_codex_reset_cache(&first)
        .expect("save first cache");
    state
        .save_codex_reset_cache(&latest)
        .expect("replace cache with latest snapshot");

    let reopened = LocalState::from_root(root);
    assert_eq!(
        reopened.load_codex_reset_cache().expect("read cache"),
        Some(latest)
    );
}

#[test]
fn legacy_codex_reset_cache_is_rejected_without_rewriting_it() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    fs::create_dir_all(&state.root).expect("create state directory");
    let legacy = r#"{"sourceUrl":"https://www.codexrunway.com/api/status.json","feedStatus":"ok","generatedAt":"2026-08-31T03:08:02.232Z","lastSuccessfulCheckAt":"2026-08-31T03:08:02.232Z","checkedAt":"2026-08-31T03:10:00.000Z","latestConfirmedReset":null,"nextScheduledReset":null,"latestRelevantTiboPost":null,"sourceWarning":null}"#;
    fs::write(state.codex_reset_cache_path(), legacy).expect("write legacy cache");

    assert_eq!(
        state.load_codex_reset_cache().unwrap_err(),
        "Codex 重置信号缓存格式无效"
    );
    assert_eq!(
        fs::read_to_string(state.codex_reset_cache_path()).expect("read legacy cache"),
        legacy
    );
}

#[test]
fn a_legacy_usage_cache_file_is_deleted_and_read_as_absent() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    fs::create_dir_all(&state.root).expect("create state directory");
    let legacy = r#"{"entries":{"profile-1":{"queryDigest":"stale","summary":{"readings":[],"at":"2026-09-06T00:00:00Z"}}}}"#;
    fs::write(state.usage_cache_path(), legacy).expect("write legacy cache");

    assert_eq!(
        state
            .load_usage_cache()
            .expect("legacy cache is dropped, not an error"),
        None
    );
    assert!(!state.usage_cache_path().exists());
}

#[test]
fn codex_quota_baseline_is_absent_without_creating_a_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));

    assert_eq!(
        state.load_codex_quota_baseline().expect("empty baseline"),
        None
    );
    assert!(!state.codex_quota_baseline_path().exists());
}

#[test]
fn codex_quota_baseline_replaces_and_persists_the_latest_read() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().join("state");
    let state = LocalState::from_root(root.clone());
    let first = stored_baseline();
    let latest = CodexQuotaBaseline {
        account_marker: Some("replacement-marker".to_string()),
        last_read: None,
        last_reset: None,
    };

    state
        .save_codex_quota_baseline(&first)
        .expect("save first baseline");
    state
        .save_codex_quota_baseline(&latest)
        .expect("replace baseline with latest read");

    let reopened = LocalState::from_root(root);
    assert_eq!(
        reopened.load_codex_quota_baseline().expect("read baseline"),
        Some(latest)
    );
}

#[test]
fn malformed_codex_quota_baseline_is_rejected_without_rewriting_it() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    fs::create_dir_all(&state.root).expect("create state directory");
    let invalid = r#"{"accountMarker":"marker","lastRead":{"at":"not-a-time","windows":[]},"lastReset":null}"#;
    fs::write(state.codex_quota_baseline_path(), invalid).expect("write invalid baseline");

    assert_eq!(
        state.load_codex_quota_baseline().unwrap_err(),
        "Codex 官方额度基线格式无效"
    );
    assert_eq!(
        fs::read_to_string(state.codex_quota_baseline_path()).expect("read invalid baseline"),
        invalid
    );
}

#[test]
fn discovery_cache_is_absent_without_creating_a_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));

    assert_eq!(state.load_discovery_cache().expect("empty cache"), None);
    assert!(!state.discovery_cache_path().exists());
}

#[test]
fn discovery_cache_persists_display_facts_and_never_credentials() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().join("state");
    let state = LocalState::from_root(root.clone());
    let report = asb_core::discovery::discover(
        &asb_core::discovery::DiscoveryPaths {
            codex: "missing-codex.toml".into(),
            codex_auth: "missing-auth.json".into(),
            claude: "claude.json".into(),
        },
        |path| {
            if path == "claude.json" {
                Ok(Some(
                        r#"{"env":{"ANTHROPIC_BASE_URL":"https://relay.internal","ANTHROPIC_AUTH_TOKEN":"TEST_CACHE_REDACTED_KEY","ANTHROPIC_MODEL":"claude-opus-4-1"}}"#
                            .to_string(),
                    ))
            } else {
                Ok(None)
            }
        },
    );
    assert!(!report.codex.exists);
    assert_eq!(report.claude_import_proposals.len(), 1);
    assert_eq!(
        report.claude_import_proposals[0].draft.api_key,
        "TEST_CACHE_REDACTED_KEY"
    );

    state
        .save_discovery_cache(&report)
        .expect("save discovery cache");
    let stored = fs::read_to_string(state.discovery_cache_path()).expect("stored text");
    assert!(!stored.contains("TEST_CACHE_REDACTED_KEY"));

    let cached = LocalState::from_root(root)
        .load_discovery_cache()
        .expect("read cache");
    assert_eq!(cached, Some(report.cached_display()));
}

#[test]
fn clear_discovery_cache_removes_the_stored_snapshot() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().join("state");
    let state = LocalState::from_root(root.clone());
    let report = asb_core::discovery::discover(
        &asb_core::discovery::DiscoveryPaths {
            codex: "c".into(),
            codex_auth: "a".into(),
            claude: "s".into(),
        },
        |_| Ok(Some(String::new())),
    );

    state
        .save_discovery_cache(&report)
        .expect("save discovery cache");
    state.clear_discovery_cache().expect("clear cache");
    state.clear_discovery_cache().expect("clear is idempotent");

    assert_eq!(
        LocalState::from_root(root)
            .load_discovery_cache()
            .expect("read cleared cache"),
        None
    );
}

#[test]
fn app_settings_default_without_creating_a_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));

    assert_eq!(
        state.get_app_settings().expect("default settings"),
        AppSettings::default()
    );
    assert!(!state.settings_path().exists());
}

#[test]
fn app_settings_persist_as_the_current_contract_only() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().join("state");
    let state = LocalState::from_root(root.clone());
    let expected = AppSettings {
        close_behavior: CloseBehavior::Exit,
        theme: ThemePreference::Dark,
        motion: MotionPreference::Reduce,
        always_on_top: true,
        launch_at_login: true,
        hardware_acceleration: false,
        interface_font: "MiSans".to_string(),
        runtime_log_level: RuntimeLogLevel::Warn,
        collapsed_usage_ids: vec!["codex-relay-a".to_string()],
    };

    state.set_app_settings(&expected).expect("save settings");
    let reopened = LocalState::from_root(root);
    assert_eq!(
        reopened.get_app_settings().expect("read settings"),
        expected
    );
    let written = fs::read_to_string(reopened.settings_path()).expect("read settings text");
    assert_eq!(
            written,
            "{\n  \"closeBehavior\": \"exit\",\n  \"theme\": \"dark\",\n  \"motion\": \"reduce\",\n  \"alwaysOnTop\": true,\n  \"launchAtLogin\": true,\n  \"hardwareAcceleration\": false,\n  \"interfaceFont\": \"MiSans\",\n  \"runtimeLogLevel\": \"warn\",\n  \"collapsedUsageIds\": [\n    \"codex-relay-a\"\n  ]\n}"
        );
    assert!(!reopened.backup_dir().exists());
}

#[test]
fn a_settings_file_without_the_usage_collapse_set_is_rejected() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    fs::create_dir_all(&state.root).expect("create state directory");
    fs::write(
            state.settings_path(),
            "{\"closeBehavior\":\"hideToTray\",\"theme\":\"dark\",\"motion\":\"reduce\",\"alwaysOnTop\":true,\"launchAtLogin\":false,\"hardwareAcceleration\":true,\"interfaceFont\":\"Noto Sans SC\",\"runtimeLogLevel\":\"info\"}",
        )
        .expect("write settings without the collapse set");

    assert_eq!(
        state
            .get_app_settings()
            .expect_err("incomplete current contract must fail"),
        "应用设置格式无效"
    );
}

#[test]
fn app_settings_reject_incomplete_unknown_and_invalid_shapes() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    fs::create_dir_all(&state.root).expect("create state directory");

    for invalid in [
            // Any previous or incomplete settings shape is rejected.
            "{\"closeBehavior\":\"hideToTray\"}",
            "{\"closeBehavior\":\"exit\",\"theme\":\"dark\",\"motion\":\"reduce\",\"alwaysOnTop\":true,\"hardwareAcceleration\":false,\"interfaceFont\":\"Noto Sans SC\",\"runtimeLogLevel\":\"info\"}",
            "{\"closeBehavior\":\"hideToTray\",\"theme\":\"system\",\"motion\":\"system\",\"alwaysOnTop\":false,\"launchAtLogin\":false,\"hardwareAcceleration\":true,\"interfaceFont\":\"Noto Sans SC\",\"runtimeLogLevel\":\"info\",\"legacy\":true}",
            "{\"closeBehavior\":\"hideToTray\",\"theme\":\"sepia\",\"motion\":\"system\",\"alwaysOnTop\":false,\"launchAtLogin\":false,\"hardwareAcceleration\":true,\"interfaceFont\":\"Noto Sans SC\",\"runtimeLogLevel\":\"info\"}",
            "{\"closeBehavior\":\"hideToTray\",\"theme\":\"system\",\"motion\":\"system\",\"alwaysOnTop\":false,\"launchAtLogin\":false,\"hardwareAcceleration\":\"false\",\"interfaceFont\":\"Noto Sans SC\",\"runtimeLogLevel\":\"info\"}",
            "{\"closeBehavior\":\"exit\",\"theme\":\"dark\",\"motion\":\"reduce\",\"alwaysOnTop\":true,\"hardwareAcceleration\":true}",
            "{\"closeBehavior\":\"exit\",\"theme\":\"dark\",\"motion\":\"reduce\",\"alwaysOnTop\":true,\"launchAtLogin\":false,\"hardwareAcceleration\":true,\"interfaceFont\":\"Noto Sans SC\",\"runtimeLogLevel\":\"verbose\"}",
            // A font name is consumed verbatim as a quoted CSS value.
            "{\"closeBehavior\":\"exit\",\"theme\":\"dark\",\"motion\":\"reduce\",\"alwaysOnTop\":true,\"launchAtLogin\":false,\"hardwareAcceleration\":true,\"interfaceFont\":\"\",\"runtimeLogLevel\":\"info\"}",
            "{\"closeBehavior\":\"exit\",\"theme\":\"dark\",\"motion\":\"reduce\",\"alwaysOnTop\":true,\"launchAtLogin\":false,\"hardwareAcceleration\":true,\"interfaceFont\":\"MiSans \",\"runtimeLogLevel\":\"info\"}",
        ] {
            fs::write(state.settings_path(), invalid).expect("write invalid settings");
            assert_eq!(state.get_app_settings().unwrap_err(), "应用设置格式无效");
        }
}

#[test]
fn app_settings_repair_replaces_an_invalid_file_with_defaults() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    fs::create_dir_all(&state.root).expect("create state directory");
    // A previous-contract shape without launchAtLogin is the repair case.
    fs::write(
            state.settings_path(),
            "{\"closeBehavior\":\"exit\",\"theme\":\"dark\",\"motion\":\"reduce\",\"alwaysOnTop\":true,\"hardwareAcceleration\":false,\"interfaceFont\":\"MiSans\",\"runtimeLogLevel\":\"warn\"}",
        )
        .expect("write stale settings");

    assert_eq!(
        state.repair_app_settings().expect("repair"),
        AppSettings::default()
    );
    assert_eq!(
        state.get_app_settings().expect("read repaired settings"),
        AppSettings::default()
    );
}

#[test]
fn app_settings_repair_refuses_a_readable_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));
    let saved = AppSettings {
        close_behavior: CloseBehavior::Exit,
        interface_font: "MiSans".to_string(),
        ..AppSettings::default()
    };
    state.set_app_settings(&saved).expect("save settings");

    assert_eq!(
        state.repair_app_settings().unwrap_err(),
        "应用设置当前可读，无需修复"
    );
    assert_eq!(state.get_app_settings().expect("read settings"), saved);
}

#[test]
fn invalid_interface_font_names_are_rejected_before_any_write() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let state = LocalState::from_root(directory.path().join("state"));

    let names = ["", "  ", "\"quoted\"", "back\\slash", "MiSans "]
        .into_iter()
        .map(str::to_string)
        .chain([core::iter::repeat('x').take(65).collect::<String>()]);
    for name in names {
        let settings = AppSettings {
            interface_font: name,
            ..AppSettings::default()
        };
        assert!(state.set_app_settings(&settings).is_err());
    }
    assert!(!state.settings_path().exists());
}
