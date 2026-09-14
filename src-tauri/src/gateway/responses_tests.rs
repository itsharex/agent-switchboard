use super::*;
use asb_core::contracts::{CodexProviderDraft, ResponsesRequestMode};
use asb_core::ownership::default_client_settings;

fn draft(
    file: &asb_core::contracts::CodexProviderFile,
    mode: ResponsesRequestMode,
) -> CodexProviderDraft {
    CodexProviderDraft {
        name: file.profile.name.clone(),
        endpoint: file.profile.endpoint.clone(),
        api_key: file.profile.api_key.clone(),
        authentication: file.profile.authentication,
        connection: file.profile.connection.clone(),
        upstream: file.profile.upstream,
        request_mode: mode,
        default_model: file.profile.default_model.clone(),
        catalog: file.profile.catalog.clone(),
        model_routes: file.profile.model_routes.clone(),
        capabilities: file.profile.capabilities.clone(),
        parameters: file.parameters.clone(),
        notes: file.notes.clone(),
        website_url: file.website_url.clone(),
        usage_query: file.usage_query.clone(),
    }
}

#[test]
fn responses_mode_owns_codex_routing_revision_and_gateway_projection() {
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&local);
    let standard = crate::gateway::server::tests::sandbox_codex_file(
        &local,
        "Responses sandbox",
        "http://127.0.0.1:18080".to_string(),
        "sandbox-secret".to_string(),
        asb_core::contracts::CodexUpstream::Responses,
    );
    let mut minimal = standard.clone();
    minimal.profile.request_mode = ResponsesRequestMode::Minimal;
    assert_ne!(
        codex_route_fingerprint(&standard).unwrap(),
        codex_route_fingerprint(&minimal).unwrap()
    );
    let projection = gateway
        .project_codex(&minimal, default_client_settings(AppKind::Codex))
        .unwrap();
    assert!(projection.warning().unwrap().contains("最小模式"));
    assert!(adapter::render("", &projection.plan)
        .unwrap()
        .contains("model_provider = \"openai\""));
    gateway.commit(&projection, || Ok(())).unwrap();
    assert_eq!(
        gateway
            .active_codex_projection(&minimal, default_client_settings(AppKind::Codex))
            .unwrap(),
        Some(projection.plan),
    );
    gateway.shutdown();
}

#[test]
fn changed_codex_request_mode_is_refused_on_restart() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let file = crate::gateway::server::tests::sandbox_codex_file(
        &local,
        "Responses sandbox",
        "http://127.0.0.1:18080".to_string(),
        "sandbox-secret".to_string(),
        asb_core::contracts::CodexUpstream::Responses,
    );
    let record = local
        .configuration()
        .list_codex_providers()
        .unwrap()
        .remove(0);
    let gateway = GatewayController::start(&local);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .unwrap();
    let target = local.target(AppKind::Codex).unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, adapter::render("", &projection.plan).unwrap()).unwrap();
    gateway.commit(&projection, || Ok(())).unwrap();
    gateway.shutdown();
    drop(gateway);

    let restarted = restart(&local);
    assert!(restarted.has_active_route_for(AppKind::Codex));
    restarted.shutdown();
    drop(restarted);

    local
        .configuration()
        .update_codex_provider(
            &record.profile.id,
            draft(&file, ResponsesRequestMode::Minimal),
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
