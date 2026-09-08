use super::*;
use asb_core::contracts::{ProviderDraft, ResponsesRequestMode};
use asb_core::ownership::{default_client_settings, default_provider_parameters};

fn draft(mode: ResponsesRequestMode) -> ProviderDraft {
    ProviderDraft {
        parameters: default_provider_parameters(AppKind::Codex),
        app: AppKind::Codex,
        route_mode: RouteMode::Custom,
        name: "Responses sandbox".to_string(),
        base_url: Some("http://127.0.0.1:18080/custom/api".to_string()),
        api_key: "sandbox-secret".to_string(),
        upstream_protocol: Some(UpstreamProtocol::Responses),
        responses_options: Some(ResponsesOptions { request_mode: mode }),
        max_output_tokens: None.into(),
        model: Some("sandbox-model".to_string()),
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

#[test]
fn responses_mode_owns_routing_fingerprint_and_gateway_projection() {
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&local);
    let standard = ProviderProfile::from_draft(
        Uuid::new_v4().to_string(),
        draft(ResponsesRequestMode::Standard),
    );
    let mut minimal = standard.clone();
    minimal.responses_options.as_mut().unwrap().request_mode = ResponsesRequestMode::Minimal;
    assert!(!is_direct(&standard));
    assert!(!is_direct(&minimal));
    assert_ne!(
        route_fingerprint(&standard).unwrap(),
        route_fingerprint(&minimal).unwrap()
    );
    let plan = SwitchPlan::direct(minimal.clone(), default_client_settings(AppKind::Codex));
    let projection = gateway.project(&plan).unwrap();
    assert!(projection.warning().unwrap().contains("最小模式"));
    assert!(!projection
        .warning()
        .unwrap()
        .contains("原生协议（Responses）不同"));
    assert!(adapter::render("", &projection.plan)
        .unwrap()
        .contains("model_provider = \"openai\""));
    gateway.commit(&projection, || Ok(())).unwrap();
    let active = gateway.active_route_projection(&plan).unwrap().unwrap();
    assert_eq!(active, projection.plan);
    gateway.shutdown();
}

#[test]
fn minimal_responses_route_is_restored_then_refused_when_mode_changes() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let record = local
        .configuration()
        .create_provider(draft(ResponsesRequestMode::Minimal))
        .unwrap();
    let gateway = GatewayController::start(&local);
    let plan = SwitchPlan::direct(
        record.profile.clone(),
        default_client_settings(AppKind::Codex),
    );
    let projection = gateway.project(&plan).unwrap();
    let target = local.target(AppKind::Codex).unwrap();
    let auth = LocalState::codex_auth_path().unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::create_dir_all(auth.parent().unwrap()).unwrap();
    fs::write(&target, adapter::render("", &projection.plan).unwrap()).unwrap();
    fs::write(
        &auth,
        r#"{"auth_mode":"chatgpt","tokens":{"access_token":"sandbox-access","refresh_token":"sandbox-refresh","id_token":"sandbox-id"}}"#,
    )
    .unwrap();
    gateway.commit(&projection, || Ok(())).unwrap();
    gateway.shutdown();
    drop(gateway);
    let restored = restart(&local);
    assert!(restored.has_active_route_for(AppKind::Codex));
    assert_eq!(
        restored.active_route_projection(&plan).unwrap(),
        Some(projection.plan)
    );
    restored.shutdown();
    drop(restored);
    local
        .configuration()
        .update_provider(
            &record.profile.id,
            draft(ResponsesRequestMode::Standard),
            &record.file_hash,
        )
        .unwrap();
    let changed = restart(&local);
    assert!(!changed.has_active_routes());
    assert_eq!(
        changed.observe(&local).status,
        GatewayStatusKind::NeedsRepair
    );
    changed.shutdown();
}

fn restart(local: &LocalState) -> GatewayController {
    for _ in 0..40 {
        let gateway = GatewayController::start(local);
        if gateway.is_listening() {
            return gateway;
        }
        gateway.shutdown();
        thread::sleep(Duration::from_millis(50));
    }
    panic!("isolated gateway did not release its listener");
}
