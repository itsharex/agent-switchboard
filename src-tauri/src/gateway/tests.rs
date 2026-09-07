use super::*;
use asb_core::contracts::{
    AuthenticationScheme, ExplicitMaxOutputTokens, ProviderDraft, UsageQuery,
};
use asb_core::ownership::default_common_settings;

fn profile(app: AppKind, protocol: UpstreamProtocol) -> ProviderProfile {
    ProviderProfile::from_draft(
        Uuid::new_v4().to_string(),
        ProviderDraft {
            app,
            route_mode: RouteMode::Custom,
            name: "Sandbox relay".to_string(),
            base_url: Some("http://127.0.0.1:18080".to_string()),
            api_key: "sandbox-upstream-key".to_string(),
            upstream_protocol: Some(protocol),
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
    SwitchPlan::direct(profile, default_common_settings(app))
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
        match GatewayController::start(local) {
            Ok(controller) => return controller,
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
    panic!("本机协议网关端口始终不可用");
}

#[test]
fn rehydrate_keeps_metadata_edited_routes_and_drops_rekeyed_ones() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let draft = |api_key: &str| ProviderDraft {
        app: AppKind::Claude,
        route_mode: RouteMode::Custom,
        name: "中转".to_string(),
        base_url: Some("http://127.0.0.1:18080".to_string()),
        api_key: api_key.to_string(),
        upstream_protocol: Some(UpstreamProtocol::ChatCompletions),
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
    let controller = GatewayController::start(&local).unwrap();
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
    third.shutdown();
}

#[test]
fn routed_projection_never_keeps_the_upstream_secret_or_endpoint() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&state).unwrap();
    let original = plan(profile(AppKind::Codex, UpstreamProtocol::AnthropicMessages));
    let projected = gateway.project(&original).unwrap();

    assert_ne!(projected.plan.profile.api_key, original.profile.api_key);
    assert_ne!(projected.plan.profile.base_url, original.profile.base_url);
    assert_eq!(
        projected.plan.client_authentication(),
        Some(AuthenticationScheme::Bearer)
    );
    assert!(projected.warning().is_some_and(|warning| {
        warning.contains("服务地址会被改写为本机协议网关地址")
            && warning.contains(&gateway.observe().unwrap().base_url)
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
    let gateway = GatewayController::start(&state).unwrap();
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
    let gateway = GatewayController::start(&state).unwrap();
    let original = plan(profile(AppKind::Codex, UpstreamProtocol::Responses));
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

    let gateway = GatewayController::start(&state).unwrap();
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
