use super::*;
use crate::contracts::{CodexChatEffortMode, CodexChatReasoning};

#[test]
fn every_pinned_codex_api_key_preset_builds_a_valid_independent_profile() {
    let summaries = list().unwrap();
    assert_eq!(summaries.len(), 85);
    let mut failures = Vec::new();
    let mut api_key_count = 0;
    for summary in summaries {
        if summary.authentication != CodexPresetAuthentication::ApiKey {
            continue;
        }
        api_key_count += 1;
        match prepare(&summary.id, "fixture-preset-key") {
            Ok(preparation) => {
                assert_eq!(preparation.draft.name, summary.name);
                assert_eq!(preparation.draft.api_key, "fixture-preset-key");
                assert_eq!(Some(preparation.draft.upstream), summary.upstream);
                let file = preparation.draft.into_file("test-preset-id".into(), 100);
                assert!(file
                    .profile
                    .catalog
                    .iter()
                    .any(|model| model.id == file.profile.default_model));
                assert_eq!(
                    file.profile.connection.endpoint_auto_select,
                    (!summary.endpoint_candidates.is_empty()).then_some(false)
                );
                let encoded = serde_json::to_string(&file).unwrap();
                let decoded: crate::contracts::CodexProviderFile =
                    serde_json::from_str(&encoded).unwrap();
                assert_eq!(decoded, file);
            }
            Err(error) => failures.push(format!("{}: {error}", summary.name)),
        }
    }
    assert_eq!(api_key_count, 83);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn presets_preserve_explicit_catalog_and_chat_dialects() {
    let summaries = list().unwrap();
    let find = |name: &str| {
        prepare(
            &summaries.iter().find(|p| p.name == name).unwrap().id,
            "test-key",
        )
        .unwrap()
        .draft
    };
    let kimi = find("Kimi");
    assert_eq!(kimi.catalog.len(), 2);
    assert_eq!(kimi.catalog[0].id, "kimi-k3");
    assert!(kimi.catalog[0].display_name.is_some());
    let zen = find("OpenCode Go");
    assert!(matches!(
        zen.capabilities.chat_reasoning,
        CodexChatReasoning::Configured {
            effort_mode: CodexChatEffortMode::Catalog,
            ..
        }
    ));
    assert_eq!(zen.catalog.len(), 6);
    let silicon = find("SiliconFlow");
    assert!(matches!(
        silicon.capabilities.chat_reasoning,
        CodexChatReasoning::Configured {
            effort_mode: CodexChatEffortMode::DeepSeek,
            ..
        }
    ));
}

#[test]
fn preset_discovery_is_credential_free_and_removes_promotion_urls() {
    for preset in list().unwrap() {
        assert!(!preset.website_url.contains('?'));
        assert!(!preset.api_key_url.as_deref().unwrap_or("").contains('?'));
        if preset.authentication != CodexPresetAuthentication::ApiKey {
            assert!(prepare(&preset.id, "not-an-oauth-token")
                .unwrap_err()
                .contains("登录"));
        } else {
            assert!(prepare(&preset.id, "").is_err());
        }
    }
    assert!(prepare("unknown", "test-key").is_err());
}
