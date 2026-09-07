//! Provider-projection round trips for Codex and Claude: switch, verify the
//! rendered candidate, and restore the original content.

mod common;

use asb_core::contracts::{AppKind, ClaudeModelSettings, ModelOptions};
use asb_core::test_support::{CLAUDE_JSON, CODEX_TOML};
use asb_switch::io::FsIo;
use asb_switch::{read_preview, sha256_hex, RecoveryOutcome};
use common::{claude_plan, codex_plan, execute, restore, setup};
use std::fs;
use std::path::Path;

#[test]
fn switch_a_to_b_to_restore_preserves_every_host_field() {
    let initial = CODEX_TOML
        .replace("relay-b.internal", "relay-a.internal")
        .replace("gpt-5.2", "gpt-5.1");
    let (_dir, target, backup_dir) = setup(AppKind::Codex, &initial);
    let original = fs::read_to_string(&target).unwrap();
    let io = FsIo;

    let plan_b = codex_plan(
        "Relay B",
        "https://relay-b.internal/v1",
        "gpt-5.2",
        "CODEX_RELAY_B_KEY",
    );
    let fp = read_preview(&io, &target, &plan_b, &backup_dir.to_string_lossy()).unwrap();
    let outcome = execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan_b,
            backup_dir: &backup_dir,
            expected_hash: &fp.content_hash,
            expected_rendered_hash: &fp.rendered_hash,
        },
    )
    .unwrap();

    assert_eq!(outcome.changed, vec![target.to_string_lossy().to_string()]);
    assert!(outcome.backup.content_hash == sha256_hex(&original));
    let switched = fs::read_to_string(&target).unwrap();
    // Credentials are not part of config.toml under the built-in `openai`
    // contract, so the redacted configuration candidate is byte-identical.
    assert_eq!(switched, fp.content);
    assert!(!fp.content.contains("CODEX_RELAY_B_KEY"));
    assert!(switched.contains("model_provider = \"openai\""));
    assert!(!switched.contains("[model_providers.OpenAi]"));
    assert!(!switched.contains("experimental_bearer_token"));
    assert!(switched.contains("openai_base_url = \"https://relay-b.internal/v1\""));
    assert!(switched.contains("model_reasoning_effort = \"xhigh\""));
    assert!(!switched.contains("CODEX_RELAY_B_KEY"));
    assert!(switched.contains("https://relay-b.internal/v1"));
    for host in ["threads = 8", "history_persistence", "trusted = true"] {
        assert!(switched.contains(host), "host field lost: {host}");
    }

    let restored = restore(&io, &outcome.backup, &target).unwrap();
    assert_eq!(restored.restored_hash, sha256_hex(&original));
    assert_eq!(fs::read_to_string(&target).unwrap(), original);
}

#[test]
fn claude_switch_round_trips_with_a_redacted_preview() {
    let (_dir, target, backup_dir) = setup(AppKind::Claude, CLAUDE_JSON);
    let io = FsIo;
    let plan = claude_plan("Relay C", "https://relay-c.internal", "claude-opus-4");

    let fp = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();
    // Anthropic Messages delivers the credential via x-api-key.
    let api_key_change = fp
        .preview
        .changes
        .iter()
        .find(|change| change.key == "env.ANTHROPIC_API_KEY")
        .expect("profile api key change");
    assert_eq!(api_key_change.after.as_deref(), Some("••••••••"));
    assert!(!fp.content.contains("test-api-key"));
    execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &fp.content_hash,
            expected_rendered_hash: &fp.rendered_hash,
        },
    )
    .unwrap();

    let text = fs::read_to_string(&target).unwrap();
    assert!(text.contains("https://relay-c.internal"));
    assert!(text.contains("claude-opus-4"));
    assert!(text.contains("Bash(npm run test:*)"));
    assert!(text.contains("\"ANTHROPIC_API_KEY\": \"test-api-key\""));
    assert!(!text.contains("ANTHROPIC_AUTH_TOKEN"));
    assert!(text.contains("\"ultracode\": true"));
}

#[test]
fn claude_one_m_switch_writes_the_wire_suffix_from_semantic_profile_state() {
    let (_dir, target, backup_dir) = setup(AppKind::Claude, CLAUDE_JSON);
    let io = FsIo;
    let mut plan = claude_plan("Relay 1M", "https://relay-c.internal", "claude-opus-4-7");
    plan.profile.model_options = Some(ModelOptions::Claude(ClaudeModelSettings {
        primary_one_m: true,
        haiku_model: Some("claude-haiku-4".into()),
        sonnet_model: Some("claude-sonnet-4-6".into()),
        sonnet_one_m: true,
        opus_model: Some("claude-opus-4-7".into()),
        opus_one_m: true,
        available_models: None,
    }));

    let preview = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();
    execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
    )
    .unwrap();

    let rendered: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
    assert_eq!(rendered["model"], "claude-opus-4-7[1m]");
    assert_eq!(
        rendered["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"],
        "claude-sonnet-4-6[1m]"
    );
    assert_eq!(
        rendered["env"]["ANTHROPIC_DEFAULT_OPUS_MODEL"],
        "claude-opus-4-7[1m]"
    );
    assert_eq!(
        rendered["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"],
        "claude-haiku-4"
    );
}

#[test]
fn switching_to_claude_profile_without_mappings_clears_the_previous_profile_mappings() {
    let (_dir, target, backup_dir) = setup(AppKind::Claude, CLAUDE_JSON);
    let io = FsIo;
    let mut plan_a = claude_plan("Aihub", "https://aihub.internal", "claude-opus-4-7");
    plan_a.profile.model_options = Some(ModelOptions::Claude(ClaudeModelSettings {
        primary_one_m: true,
        haiku_model: Some("claude-haiku-4".into()),
        sonnet_model: Some("claude-sonnet-4-6".into()),
        sonnet_one_m: true,
        opus_model: Some("claude-opus-4-7".into()),
        opus_one_m: true,
        available_models: Some(vec!["claude-opus-4-7".into()]),
    }));
    let preview_a = read_preview(&io, &target, &plan_a, &backup_dir.to_string_lossy()).unwrap();
    execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan_a,
            backup_dir: &backup_dir,
            expected_hash: &preview_a.content_hash,
            expected_rendered_hash: &preview_a.rendered_hash,
        },
    )
    .unwrap();

    let plan_b = claude_plan(
        "AnyRouter",
        "https://anyrouter.internal",
        "claude-sonnet-4-6",
    );
    let preview_b = read_preview(&io, &target, &plan_b, &backup_dir.to_string_lossy()).unwrap();
    execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan_b,
            backup_dir: &backup_dir,
            expected_hash: &preview_b.content_hash,
            expected_rendered_hash: &preview_b.rendered_hash,
        },
    )
    .unwrap();

    let rendered: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
    assert_eq!(rendered["model"], "claude-sonnet-4-6");
    for key in [
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
    ] {
        assert!(
            rendered["env"].get(key).is_none(),
            "{key} leaked from Aihub"
        );
    }
    assert!(rendered.get("availableModels").is_none());
}

#[test]
fn switching_to_codex_profile_without_one_m_clears_the_previous_profile_window() {
    let (_dir, target, backup_dir) = setup(AppKind::Codex, CODEX_TOML);
    let io = FsIo;
    let mut plan_a = codex_plan("Aihub", "https://aihub.internal/v1", "gpt-5.2", "AIHUB_KEY");
    plan_a.profile.model_options = Some(ModelOptions::Codex(asb_core::CodexModelSettings {
        context_window: Some(1_000_000),
    }));
    let preview_a = read_preview(&io, &target, &plan_a, &backup_dir.to_string_lossy()).unwrap();
    execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan_a,
            backup_dir: &backup_dir,
            expected_hash: &preview_a.content_hash,
            expected_rendered_hash: &preview_a.rendered_hash,
        },
    )
    .unwrap();

    let plan_b = codex_plan(
        "AnyRouter",
        "https://anyrouter.internal/v1",
        "gpt-5.3",
        "ANYROUTER_KEY",
    );
    let preview_b = read_preview(&io, &target, &plan_b, &backup_dir.to_string_lossy()).unwrap();
    execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan_b,
            backup_dir: &backup_dir,
            expected_hash: &preview_b.content_hash,
            expected_rendered_hash: &preview_b.rendered_hash,
        },
    )
    .unwrap();

    let rendered = fs::read_to_string(&target).unwrap();
    assert!(!rendered.contains("model_context_window"));
    assert!(rendered.contains("model_reasoning_effort = \"xhigh\""));
}

#[test]
fn outcome_reports_lock_changes_warnings_backup_and_recovery() {
    let (_dir, target, backup_dir) = setup(AppKind::Claude, CLAUDE_JSON);
    let io = FsIo;
    let plan = claude_plan("Relay C", "https://relay-c.internal", "claude-opus-4");
    let fp = read_preview(&io, &target, &plan, &backup_dir.to_string_lossy()).unwrap();

    let outcome = execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backup_dir,
            expected_hash: &fp.content_hash,
            expected_rendered_hash: &fp.rendered_hash,
        },
    )
    .unwrap();

    assert!(matches!(outcome.lock, asb_core::LockStatus::Free));
    assert!(!outcome.acquired_at.is_empty());
    assert_eq!(outcome.changed.len(), 1);
    // The sample carries env.ANTHROPIC_MODEL, which overrides `model`; the
    // adapter removes it and warns.
    assert!(outcome
        .warnings
        .iter()
        .any(|w| w.contains("ANTHROPIC_MODEL")));
    assert!(Path::new(&outcome.backup.backup_path).exists());
    assert!(matches!(outcome.recovery, RecoveryOutcome::NotNeeded));
    // Preview in the outcome matches what the user confirmed.
    assert_eq!(outcome.preview, fp.preview);
    // The final hash describes the content that is now live.
    assert_eq!(
        outcome.final_hash,
        sha256_hex(&fs::read_to_string(&target).unwrap())
    );
}
