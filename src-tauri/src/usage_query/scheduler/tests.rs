use super::*;
use asb_core::contracts::{ProviderDraft, UpstreamProtocol, UsageQuery};

fn claude(state: &LocalState) -> ProviderProfile {
    state.configuration().create_provider(ProviderDraft {
        app: AppKind::Claude,
        route_mode: RouteMode::Custom,
        name: "isolated-claude".into(),
        model: Some("test-model".into()),
        base_url: Some("https://example.invalid".into()),
        api_key: "isolated-key".into(),
        authentication: None,
        upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
        claude_fragment: Default::default(),
        connection: Default::default(),
        responses_options: None,
        max_output_tokens: None.into(),
        model_options: None,
        notes: None,
        website_url: None,
        display: None,
        official_quota_refresh_interval_minutes: None,
        usage_query: Some(UsageQuery::Declarative {
            url: "{{baseUrl}}/usage".into(), remaining_path: Some("/remaining".into()),
            used_path: None, total_path: None, unit: None, refresh_interval_minutes: 0,
        }),
    }).unwrap().profile
}

#[test]
fn codex_store_errors_do_not_block_claude_cache_reads_or_scheduling() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let claude = claude(&state);
    let (official, _) = state.configuration().ensure_codex_official_record().unwrap();
    let codex = state.configuration().providers_dir(AppKind::Codex);
    std::fs::write(codex.join("invalid.json"), "invalid").unwrap();
    std::fs::write(codex.join("official").join(format!("{}.json", official.profile.id)), "invalid").unwrap();
    let scheduled = profiles(&state);
    assert_eq!(scheduled.len(), 1);
    assert_eq!(scheduled[0].id, claude.id);
    let resolved = usage_profile(&state, AppKind::Claude, &claude.id).unwrap();
    assert_eq!(resolved.id, claude.id);
    // Zero interval must return before any upstream access or cache write.
    assert!(!execute_once(&state, &resolved, false).unwrap());
    assert!(!state.root().join("usage-cache.json").exists());
}

#[test]
fn claude_store_errors_do_not_block_manual_only_official_profile() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let (official, _) = state.configuration().ensure_codex_official_record().unwrap();
    let claude = state.configuration().providers_dir(AppKind::Claude);
    std::fs::write(claude.join("invalid.json"), "invalid").unwrap();
    let scheduled = profiles(&state);
    assert_eq!(scheduled.len(), 1);
    assert_eq!(scheduled[0].id, official.profile.id);
    // This path must not resolve native credentials when automatic refresh is off.
    assert!(!crate::commands::quota_refresh::refresh(&state, &official.profile.id, false).unwrap());
}
