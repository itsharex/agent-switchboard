use super::activation::{activate_staged, Activation};
use super::*;
use crate::config_store::ConfigStore;
use asb_core::contracts::{
    AppKind, CodexCapabilities, CodexCatalogEntry, CodexEndpoint, CodexModelRoute,
    CodexProviderDraft, CodexUpstream,
};
use std::fs;
use std::path::Path;

fn codex_draft() -> CodexProviderDraft {
    CodexProviderDraft {
        name: "Codex relay".to_string(),
        endpoint: CodexEndpoint("https://relay.example/v1".to_string()),
        api_key: "fixture-key".to_string(),
        upstream: CodexUpstream::Responses,
        request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
        default_model: "codex-test".to_string(),
        catalog: vec![CodexCatalogEntry {
            id: "codex-test".to_string(),
            context_window: 128_000,
            max_output_tokens: 16_384,
            function_tools: true,
            custom_tools: true,
            tool_search: true,
            reasoning: true,
            default_reasoning_level: asb_core::contracts::CodexReasoningLevel::High,
            supported_reasoning_levels: vec![
                asb_core::contracts::CodexReasoningLevel::None,
                asb_core::contracts::CodexReasoningLevel::High,
            ],
            images: false,
            compact: true,
        }],
        model_routes: vec![CodexModelRoute {
            client_model: "codex-test".to_string(),
            upstream_model: "vendor-codex".to_string(),
        }],
        capabilities: CodexCapabilities {
            responses: true,
            compact: true,
            models: true,
            chat_completions: false,
            alpha_search: false,
            image_generation: false,
            image_edit: false,
            function_tools: true,
            custom_tools: true,
            tool_search: true,
            reasoning: true,
            chat_reasoning: asb_core::contracts::CodexChatReasoning::Unsupported,
        },
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Codex),
        notes: None,
        website_url: None,
        usage_query: None,
    }
}

fn live_snapshot() -> (tempfile::TempDir, ConfigStore, ConfigurationSnapshot) {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    store.create_codex_provider(codex_draft()).unwrap();
    let snapshot = read_configuration_snapshot(&store).unwrap();
    (directory, store, snapshot)
}

#[test]
fn snapshot_round_trips_the_raw_codex_file() {
    let (_directory, store, snapshot) = live_snapshot();
    enable_snapshot(&store, &snapshot).unwrap();
    assert_eq!(read_configuration_snapshot(&store).unwrap(), snapshot);
    let raw = std::fs::read_to_string(
        store
            .providers_dir(AppKind::Codex)
            .join(format!("{}.json", snapshot.codex_providers[0].profile.id)),
    )
    .unwrap();
    assert!(raw.contains("schemaVersion"));
    assert!(raw.contains("catalog"));
    assert!(!raw.contains("routeMode"));
}

#[test]
fn current_snapshot_rejects_old_and_generic_codex_shapes() {
    let (_directory, _store, snapshot) = live_snapshot();
    let mut old = serde_json::to_value(&snapshot).unwrap();
    old["schemaVersion"] = serde_json::json!(5);
    assert!(decode_cloud_backup_snapshot(&serde_json::to_vec(&old).unwrap()).is_err());
    let mut generic = serde_json::to_value(&snapshot).unwrap();
    generic["codexProviders"][0] = serde_json::json!({
        "id": "00000000-0000-0000-0000-000000000001",
        "routeMode": "custom"
    });
    assert!(decode_cloud_backup_snapshot(&serde_json::to_vec(&generic).unwrap()).is_err());
}

#[test]
fn invalid_snapshot_does_not_replace_live_configuration() {
    let (_directory, store, mut snapshot) = live_snapshot();
    snapshot.codex_providers[0].profile.id = "not-a-uuid".to_string();
    assert!(enable_snapshot(&store, &snapshot).is_err());
    assert_eq!(
        read_configuration_snapshot(&store)
            .unwrap()
            .provider_count(),
        1
    );
}

#[test]
fn failed_swap_restores_the_original_live_directory() {
    let directory = tempfile::tempdir().unwrap();
    let live = directory.path().join("configuration");
    let staged = directory.path().join("staging").join("configuration");
    fs::create_dir_all(&live).unwrap();
    fs::create_dir_all(&staged).unwrap();
    fs::write(live.join("marker"), "old").unwrap();
    fs::write(staged.join("marker"), "new").unwrap();
    let mut calls = 0;
    let mut rename = |from: &Path, to: &Path| {
        calls += 1;
        if calls == 2 {
            return Err(std::io::Error::other("injected staged activation failure"));
        }
        fs::rename(from, to)
    };
    assert!(matches!(
        activate_staged(
            &live,
            &staged,
            &directory.path().join("retired"),
            &mut rename
        ),
        Activation::Restored(_)
    ));
    assert_eq!(fs::read_to_string(live.join("marker")).unwrap(), "old");
}
