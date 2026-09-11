use super::*;

fn saved_route(local: &LocalState, gateway: &GatewayController, app: AppKind) -> String {
    let projection = match app {
        AppKind::Codex => {
            let file = crate::gateway::server::tests::sandbox_codex_file(
                local,
                "restore fixture",
                "http://127.0.0.1:18080".to_string(),
                "fixture-key".to_string(),
                asb_core::contracts::CodexUpstream::Responses,
            );
            gateway
                .project_codex(&file, asb_core::ownership::default_client_settings(app))
                .unwrap()
        }
        AppKind::Claude => {
            let profile = local
                .configuration()
                .create_provider(claude_gateway_draft("fixture-key"))
                .unwrap()
                .profile;
            gateway.project(&plan(profile)).unwrap()
        }
    };
    adapter::render(
        if app == AppKind::Codex {
            "# keep this comment\nmodel = 'old-model'\n"
        } else {
            "{}"
        },
        &projection.plan,
    )
    .unwrap()
}

#[test]
fn restore_rebuilds_only_the_port_and_preserves_the_route_identity() {
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&local);
    for app in [AppKind::Codex, AppKind::Claude] {
        let original = saved_route(&local, &gateway, app);
        let base = gateway.configured_base_url();
        let archived = original.replace(&base, "http://127.0.0.1:12345");
        let rebuilt = gateway.prepare_restored(&local, app, &archived).unwrap();
        assert_eq!(rebuilt, original);
        let invalid = match app {
            AppKind::Codex => archived.replace("asb_codex_", "asb_codex_0"),
            AppKind::Claude => archived.replace("asb_local_", "asb_local_0"),
        };
        assert!(gateway.prepare_restored(&local, app, &invalid).is_err());
    }
    gateway.shutdown();
}

#[test]
fn restore_requires_a_live_unblocked_listener_for_third_party() {
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&local);
    let saved = saved_route(&local, &gateway, AppKind::Codex);
    gateway.block_port_change(port_change::BlockedPortChange {
        from_port: gateway.configured_port(),
        to_port: 12345,
        apps: vec![AppKind::Codex],
        reason: "fixture recovery".into(),
    });
    assert!(gateway
        .prepare_restored(&local, AppKind::Codex, &saved)
        .is_err());
    assert!(gateway
        .prepare_restored(&local, AppKind::Codex, "model_provider = 'openai'")
        .is_ok());
    gateway.clear_blocked_port_change().unwrap();
    gateway.shutdown();
    assert!(gateway
        .prepare_restored(&local, AppKind::Codex, &saved)
        .is_err());
}

#[test]
fn restore_rejects_retired_provider_and_reserved_provider_overrides() {
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&local);
    for config in [
        "model_provider = 'OpenAI'",
        "model_provider = 12",
        "[model_providers.openai]\nname='openai'",
        "model_provider = 'agent_switchboard'",
        "model_provider = 'openai'\n[model_providers.agent_switchboard]\nbase_url = 'https://old.example'",
        "model_provider = 'openai'\n[model_providers.OpenAi]\nbase_url = 'https://old.example'",
    ] {
        assert!(gateway
            .prepare_restored(&local, AppKind::Codex, config)
            .is_err());
    }
    gateway.shutdown();
}

#[test]
fn damaged_configuration_does_not_panic_during_dependency_observation() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    for app in [AppKind::Codex, AppKind::Claude] {
        let target = local.target(app).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, "{ invalid config [").unwrap();
    }
    let gateway = GatewayController::start(&local);
    assert!(!points_at_gateway_files(&local));
    assert!(!gateway.has_gateway_dependency(&local));
    assert_eq!(gateway.observe(&local).status, GatewayStatusKind::Standby);
    gateway.shutdown();
}
