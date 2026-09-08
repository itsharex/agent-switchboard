use super::*;
use asb_core::contracts::{ExplicitMaxOutputTokens, ProviderDraft, UsageQuery, WriteOperation};
use asb_core::ownership::default_client_settings;

fn profile(app: AppKind, protocol: UpstreamProtocol) -> ProviderProfile {
    ProviderProfile::from_draft(
        Uuid::new_v4().to_string(),
        ProviderDraft {
            parameters: asb_core::ownership::default_provider_parameters(app),
            app,
            route_mode: RouteMode::Custom,
            name: "Sandbox relay".to_string(),
            base_url: Some("http://127.0.0.1:18080".to_string()),
            api_key: "sandbox-upstream-key".to_string(),
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
fn route_fingerprint_ignores_metadata_and_client_projection_fields() {
    let base = profile(AppKind::Codex, UpstreamProtocol::AnthropicMessages);
    let mut metadata = base.clone();
    metadata.name = "改名".into();
    metadata.notes = Some("备注".into());
    metadata.website_url = Some("https://example.com".into());
    metadata.model = Some("another-model".into());
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
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
        app: AppKind::Claude,
        route_mode: RouteMode::Custom,
        name: "中转".to_string(),
        base_url: Some("http://127.0.0.1:18080".to_string()),
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
    let original = plan(profile(AppKind::Codex, UpstreamProtocol::AnthropicMessages));
    let projected = gateway.project(&original).unwrap();

    assert_eq!(projected.plan.profile, original.profile);
    assert!(projected.plan.client_api_key().is_empty());
    assert_ne!(
        projected.plan.client_base_url(),
        original.profile.base_url.as_deref()
    );
    assert_eq!(projected.plan.client_authentication(), None);
    assert!(projected.warning().is_some_and(|warning| {
        warning.contains("服务地址会被改写为本机协议网关地址")
            && warning.contains("127.0.0.1")
            && warning.contains("网页搜索会在此路由中关闭")
            && warning.contains("reasoning.encrypted_content")
    }));
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
fn direct_projection_does_not_create_a_gateway_warning() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&state);
    let mut official = profile(AppKind::Codex, UpstreamProtocol::Responses);
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
fn invalid_gateway_state_is_quarantined_before_a_fresh_listener_starts() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let path = state.gateway_state_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "{not-json").unwrap();

    let gateway = GatewayController::start(&state);
    assert!(!gateway.has_active_routes());
    let restored: GatewayStateFile =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(restored.version, STATE_VERSION);
    assert!(fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with("gateway.invalid.")));
    gateway.shutdown();
}

#[test]
fn gateway_state_rejects_a_persisted_port_outside_the_current_contract() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let path = state.gateway_state_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut invalid = fresh_state();
    invalid.port = 1;
    write_state(&path, &invalid).unwrap();

    let gateway = GatewayController::start(&state);
    let replacement: GatewayStateFile =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_ne!(replacement.port, 1);
    assert!(fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with("gateway.invalid.")));
    gateway.shutdown();
}

#[test]
fn custom_ports_are_validated_against_the_registered_range() {
    assert_eq!(DEFAULT_GATEWAY_PORT, 47821);
    assert!(validate_custom_port(0).is_err());
    assert!(validate_custom_port(1023).is_err());
    assert!(validate_custom_port(MIN_CUSTOM_PORT).is_ok());
    assert!(validate_custom_port(MAX_CUSTOM_PORT).is_ok());
}

/// A port held by another process degrades into a visible conflict state —
/// the controller stays usable, read-only facts keep working, routed writes
/// are refused, and an explicit retry rebinds once the port is free.
#[test]
fn a_conflicted_port_keeps_the_controller_usable_until_retry_succeeds() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let blocker = Arc::new(Server::http(("127.0.0.1", 0)).unwrap());
    let port = blocker.server_addr().to_ip().unwrap().port();
    let path = state.gateway_state_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut seed = fresh_state();
    seed.port = port;
    write_state(&path, &seed).unwrap();

    let gateway = GatewayController::start(&state);
    assert!(!gateway.is_listening());
    let observation = gateway.observe(&state);
    assert_eq!(observation.configured_port, port);
    assert_eq!(observation.listening_port, None);
    assert_eq!(observation.base_url, None);
    assert_eq!(observation.status, GatewayStatusKind::PortConflict);
    let failure = observation.failure.expect("conflict reports failure");
    assert_eq!(failure.kind, GatewayFailureKind::PortInUse);
    assert_eq!(failure.port, port);
    let routed = gateway.project(&plan(profile(
        AppKind::Claude,
        UpstreamProtocol::ChatCompletions,
    )));
    assert!(routed.is_err());

    drop(blocker);
    let mut rebound = None;
    for _ in 0..40 {
        let candidate = gateway.retry_bind(&state);
        if candidate.listening_port.is_some() {
            rebound = Some(candidate);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let observation = rebound.expect("retry eventually binds the released port");
    assert_eq!(observation.listening_port, Some(port));
    assert_eq!(observation.status, GatewayStatusKind::Standby);
    gateway.shutdown();
}

/// Replacing corrupted state while a client still points at a loopback
/// gateway endpoint must surface the explicit repair state instead of a
/// healthy standby.
#[test]
fn replaced_state_with_a_gateway_dependent_client_requires_explicit_repair() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let target = state.target(AppKind::Claude).unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(
        &target,
        r#"{"env":{"ANTHROPIC_BASE_URL":"http://127.0.0.1:53176","ANTHROPIC_AUTH_TOKEN":"asb_local_deadbeef"}}"#,
    )
    .unwrap();
    let path = state.gateway_state_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "{not-json").unwrap();

    let gateway = GatewayController::start(&state);
    assert_eq!(
        gateway.observe(&state).status,
        GatewayStatusKind::NeedsRepair
    );
    gateway.shutdown();
}

/// Exit decisions must consult persisted dependency and the live client
/// files: a failed listener with a dependent client still blocks quitting.
#[test]
fn gateway_dependency_outlives_a_failed_listener() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let target = state.target(AppKind::Claude).unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(
        &target,
        r#"{"env":{"ANTHROPIC_BASE_URL":"http://127.0.0.1:53176","ANTHROPIC_AUTH_TOKEN":"asb_local_deadbeef"}}"#,
    )
    .unwrap();
    let blocker = Arc::new(Server::http(("127.0.0.1", 0)).unwrap());
    let port = blocker.server_addr().to_ip().unwrap().port();
    let path = state.gateway_state_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut seed = fresh_state();
    seed.port = port;
    write_state(&path, &seed).unwrap();

    let gateway = GatewayController::start(&state);
    assert!(!gateway.is_listening());
    assert!(!gateway.has_active_routes());
    assert!(gateway.has_gateway_dependency(&state));
    gateway.shutdown();
}

#[path = "tests/port_projection.rs"]
mod port_projection;

#[path = "tests/restore.rs"]
mod restore;

fn claude_gateway_draft(api_key: &str) -> asb_core::contracts::ProviderDraft {
    asb_core::contracts::ProviderDraft {
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
        app: AppKind::Claude,
        route_mode: RouteMode::Custom,
        name: "中转".to_string(),
        base_url: Some("http://127.0.0.1:18080".to_string()),
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
    }
}
