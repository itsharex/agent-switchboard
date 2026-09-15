use super::*;
use asb_core::contracts::{ExplicitMaxOutputTokens, ProviderDraft, UsageQuery, WriteOperation};
use asb_core::ownership::default_client_settings;

fn profile(app: AppKind, protocol: UpstreamProtocol) -> ProviderProfile {
    ProviderProfile::from_draft(
        Uuid::new_v4().to_string(),
        ProviderDraft {
            authentication: None,
            parameters: asb_core::ownership::default_provider_parameters(app),
            claude_fragment: Default::default(),
            app,
            route_mode: RouteMode::Custom,
            name: "Sandbox relay".to_string(),
            base_url: Some("http://127.0.0.1:18080".to_string()),
            connection: Default::default(),
            api_key: "sandbox-upstream-key".to_string(),
            display: None,
            upstream_protocol: Some(protocol),
            responses_options: (Some(protocol)
                == Some(asb_core::contracts::UpstreamProtocol::Responses))
            .then_some(asb_core::contracts::ResponsesOptions {
                request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
            }),
            max_output_tokens: ((app == AppKind::Codex
                && protocol == UpstreamProtocol::AnthropicMessages)
                .then_some(8_192))
            .into(),
            model: Some("sandbox-model".to_string()),
            model_options: None,
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        },
    )
}

fn plan(profile: ProviderProfile) -> SwitchPlan {
    let app = profile.app;
    SwitchPlan::direct(profile, default_client_settings(app))
}

#[test]
fn route_fingerprint_ignores_metadata_but_tracks_live_model_changes() {
    let base = profile(AppKind::Claude, UpstreamProtocol::AnthropicMessages);
    let mut metadata = base.clone();
    metadata.name = "改名".into();
    metadata.notes = Some("备注".into());
    metadata.website_url = Some("https://example.com".into());
    metadata.official_quota_refresh_interval_minutes = Some(15);
    metadata.usage_query = Some(UsageQuery::Declarative {
        url: "https://example.com/balance".into(),
        remaining_path: None,
        used_path: None,
        total_path: None,
        unit: None,
        refresh_interval_minutes: 0,
    });
    assert_eq!(
        route_fingerprint(&base).unwrap(),
        route_fingerprint(&metadata).unwrap()
    );

    let mut moved_model = base.clone();
    moved_model.model = Some("another-model".into());
    assert_ne!(
        route_fingerprint(&base).unwrap(),
        route_fingerprint(&moved_model).unwrap()
    );

    let mut rekeyed = base.clone();
    rekeyed.api_key = "another-key".into();
    assert_ne!(
        route_fingerprint(&base).unwrap(),
        route_fingerprint(&rekeyed).unwrap()
    );
    let mut moved = base.clone();
    moved.base_url = Some("http://127.0.0.1:18081".into());
    assert_ne!(
        route_fingerprint(&base).unwrap(),
        route_fingerprint(&moved).unwrap()
    );
}

/// The persisted listener port is reused, so a prior controller must have
/// fully released it before the next one can bind.
fn restart_controller(local: &LocalState) -> GatewayController {
    for _ in 0..40 {
        let controller = GatewayController::start(local);
        if controller.is_listening() {
            return controller;
        }
        controller.shutdown();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("本机协议网关端口始终不可用");
}

#[test]
fn rehydrate_keeps_metadata_edited_routes_and_drops_rekeyed_ones() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let draft = |api_key: &str| ProviderDraft {
        authentication: None,
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
        claude_fragment: Default::default(),
        app: AppKind::Claude,
        route_mode: RouteMode::Custom,
        name: "中转".to_string(),
        base_url: Some("http://127.0.0.1:18080".to_string()),
        connection: Default::default(),
        api_key: api_key.to_string(),
        upstream_protocol: Some(UpstreamProtocol::ChatCompletions),
        responses_options: None,
        max_output_tokens: ExplicitMaxOutputTokens::none(),
        model: Some("model-a".to_string()),
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
        display: None,
    };
    let record = local
        .configuration()
        .create_provider(draft("upstream-key"))
        .unwrap();
    let controller = GatewayController::start(&local);
    let original = plan(record.profile.clone());
    let projection = controller.project(&original).unwrap();
    let target = local.target(AppKind::Claude).unwrap();
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(
        &target,
        asb_core::adapter::render("{}", &projection.plan).unwrap(),
    )
    .unwrap();
    controller.commit(&projection, || Ok(())).unwrap();
    assert!(controller.has_active_routes());
    controller.shutdown();
    drop(controller);

    // Metadata-only edits keep the capability token, so a restart
    // restores the same route without any client rewrite.
    let mut renamed = draft("upstream-key");
    renamed.name = "改名".into();
    renamed.notes = Some("备注".into());
    let renamed_record = local
        .configuration()
        .update_provider(&record.profile.id, renamed, &record.file_hash)
        .unwrap();
    let restarted = restart_controller(&local);
    assert!(restarted.has_active_routes());
    restarted.shutdown();
    drop(restarted);

    // Replacing a routing parameter rotates the token. Old derived state
    // gets no compatibility path: the stale route is refused and an
    // explicit re-apply is required.
    let mut rekeyed = draft("other-key");
    rekeyed.name = renamed_record.profile.name.clone();
    local
        .configuration()
        .update_provider(&record.profile.id, rekeyed, &renamed_record.file_hash)
        .unwrap();
    let third = restart_controller(&local);
    assert!(!third.has_active_routes());
    assert_eq!(
        third.observe(&local).status,
        GatewayStatusKind::NeedsRepair,
        "a stale gateway client must remain visible after restart"
    );
    third.shutdown();
}

#[test]
fn routed_projection_preserves_upstream_and_hides_client_secrets() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&state);
    let file = crate::gateway::server::tests::sandbox_codex_file(
        &state,
        "Anthropic relay",
        "http://127.0.0.1:18080".to_string(),
        "sandbox-upstream-key".to_string(),
        asb_core::contracts::CodexUpstream::AnthropicMessages,
    );
    let original = file.client_projection().into_profile(AppKind::Codex);
    let projected = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .unwrap();

    assert_eq!(projected.plan.profile, original);
    assert!(projected.plan.client_api_key().is_empty());
    assert_ne!(
        projected.plan.client_base_url(),
        original.base_url.as_deref()
    );
    assert_eq!(projected.plan.client_authentication(), None);
    assert_eq!(
        projected.warning(),
        Some("Codex 第三方模型请求将经过本机协议网关")
    );
    gateway.shutdown();
}

#[test]
fn codex_provider_switch_keeps_the_client_entry_and_replaces_the_route_revision() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&state);
    let first = crate::gateway::server::tests::sandbox_codex_file(
        &state,
        "first relay",
        "http://127.0.0.1:18080".to_string(),
        "first-upstream-key".to_string(),
        asb_core::contracts::CodexUpstream::Responses,
    );
    let second = crate::gateway::server::tests::sandbox_codex_file(
        &state,
        "second relay",
        "http://127.0.0.1:18081".to_string(),
        "second-upstream-key".to_string(),
        asb_core::contracts::CodexUpstream::Responses,
    );
    let settings = default_client_settings(AppKind::Codex);
    let first_projection = gateway.project_codex(&first, settings.clone()).unwrap();
    let second_projection = gateway.project_codex(&second, settings.clone()).unwrap();
    assert_eq!(
        first_projection.plan.client_base_url(),
        second_projection.plan.client_base_url(),
        "Codex must not need a config rewrite for A to B"
    );
    gateway.commit(&first_projection, || Ok(())).unwrap();
    gateway.commit(&second_projection, || Ok(())).unwrap();
    assert_eq!(
        gateway
            .active_codex_projection(&second, settings)
            .unwrap()
            .and_then(|plan| plan.client_base_url().map(str::to_string)),
        second_projection.plan.client_base_url().map(str::to_string)
    );
    assert!(gateway
        .active_codex_projection(&first, default_client_settings(AppKind::Codex))
        .unwrap()
        .is_none());
    gateway.shutdown();
}

#[test]
fn claude_routed_projection_explains_the_loopback_rewrite_without_codex_caveats() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&state);
    let original = plan(profile(AppKind::Claude, UpstreamProtocol::ChatCompletions));
    let projected = gateway.project(&original).unwrap();

    let warning = projected.warning().expect("routed projection warns");
    assert!(warning.contains("Claude Code 原生协议（Anthropic Messages）"));
    assert!(warning.contains("Chat Completions"));
    assert!(warning.contains("服务地址会被改写为本机协议网关地址"));
    assert!(!warning.contains("网页搜索"));
    gateway.shutdown();
}

#[test]
fn direct_claude_projection_does_not_create_a_gateway_warning() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&state);
    let mut official = profile(AppKind::Claude, UpstreamProtocol::AnthropicMessages);
    official.route_mode = RouteMode::Official;
    official.base_url = None;
    official.api_key.clear();
    official.upstream_protocol = None;
    official.responses_options = None;
    official.model = None;
    let original = plan(official);
    let projected = gateway.project(&original).unwrap();

    assert_eq!(projected.plan, original);
    assert!(projected.warning().is_none());
    gateway.shutdown();
}

#[test]
fn capability_comparison_has_no_early_success_path() {
    assert!(constant_time_equal(b"abc", b"abc"));
    assert!(!constant_time_equal(b"abc", b"abd"));
    assert!(!constant_time_equal(b"abc", b"ab"));
}

#[test]
fn temp_two_codex_profiles_reconcile_restored_probe() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&local);
    let first = crate::gateway::server::tests::sandbox_codex_file(
        &local,
        "first relay",
        "http://127.0.0.1:18080".to_string(),
        "first-upstream-key".to_string(),
        asb_core::contracts::CodexUpstream::Responses,
    );
    let _second = crate::gateway::server::tests::sandbox_codex_file(
        &local,
        "second relay",
        "http://127.0.0.1:18081".to_string(),
        "second-upstream-key".to_string(),
        asb_core::contracts::CodexUpstream::Responses,
    );
    let projection = gateway
        .project_codex(&first, default_client_settings(AppKind::Codex))
        .unwrap();
    let target = local.target(AppKind::Codex).unwrap();
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    let rendered = asb_core::adapter::render(
        "# keep this comment\nmodel = 'old-model'\n",
        &projection.plan,
    )
    .unwrap();
    std::fs::write(&target, &rendered).unwrap();
    let reconciled = gateway.reconcile_restored(&local, AppKind::Codex, || Ok(()));
    println!("PROBE reconcile_restored = {reconciled:?}");
    let prepared = gateway.prepare_restored(&local, AppKind::Codex, &rendered);
    println!("PROBE prepare_restored = {:?}", prepared.is_ok());
    let port_change = crate::gateway::port_change::prepare(
        &gateway,
        &local,
        &crate::gateway::PortChangePreparations::default(),
        47899,
    );
    println!("PROBE port_change = {:?}", port_change.err());
    gateway.shutdown();
}

#[path = "tests/port_projection.rs"]
mod port_projection;

#[path = "tests/restore.rs"]
mod restore;

#[path = "tests/state_repair.rs"]
mod state_repair;

#[path = "tests/listener.rs"]
mod listener;

fn claude_gateway_draft(api_key: &str) -> asb_core::contracts::ProviderDraft {
    asb_core::contracts::ProviderDraft {
        authentication: None,
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
        claude_fragment: Default::default(),
        app: AppKind::Claude,
        route_mode: RouteMode::Custom,
        name: "中转".to_string(),
        base_url: Some("http://127.0.0.1:18080".to_string()),
        connection: Default::default(),
        api_key: api_key.to_string(),
        upstream_protocol: Some(UpstreamProtocol::ChatCompletions),
        responses_options: None,
        max_output_tokens: asb_core::contracts::ExplicitMaxOutputTokens::none(),
        model: Some("model-a".to_string()),
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
        display: None,
    }
}
