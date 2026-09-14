use super::*;

#[test]
fn codex_restore_and_port_change_select_one_saved_revision() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&local);
    let first = saved_route(&local, &gateway, AppKind::Codex);
    let second = saved_route(&local, &gateway, AppKind::Codex);
    assert_ne!(first, second);
    let records = local.configuration().list_codex_providers().unwrap();
    let first_id = records[0].profile.id.clone();
    let second_id = records[1].profile.id.clone();
    let target = local.target(AppKind::Codex).unwrap();
    fs::write(&target, &first).unwrap();

    assert_eq!(
        gateway
            .prepare_restored(&local, AppKind::Codex, &first)
            .unwrap(),
        first
    );
    gateway
        .reconcile_restored(&local, AppKind::Codex, || Ok(()))
        .unwrap();
    assert_eq!(
        gateway.active_profile_id(AppKind::Codex, &first).unwrap(),
        Some(first_id.clone())
    );
    let preparations = PortChangePreparations::default();
    let prepared = super::port_projection::prepare_available_port(&gateway, &local, &preparations);
    assert_eq!(prepared.clients.len(), 1);
    assert_eq!(prepared.clients[0].profile_id, first_id);

    fs::write(&target, &second).unwrap();
    gateway
        .reconcile_restored(&local, AppKind::Codex, || Ok(()))
        .unwrap();
    assert_eq!(
        gateway.active_profile_id(AppKind::Codex, &second).unwrap(),
        Some(second_id)
    );
    gateway.shutdown();
}

#[test]
fn codex_restore_rejects_changed_deleted_and_unidentified_routes() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&local);
    let saved = saved_route(&local, &gateway, AppKind::Codex);
    let record = local
        .configuration()
        .list_codex_providers()
        .unwrap()
        .remove(0);
    let mut unidentified = saved.parse::<toml_edit::DocumentMut>().unwrap();
    unidentified.remove("model_catalog_json");
    assert!(gateway
        .prepare_restored(&local, AppKind::Codex, &unidentified.to_string())
        .is_err());

    let mut changed = local
        .configuration()
        .find_codex_provider_file(&record.profile.id)
        .unwrap();
    changed.profile.api_key = "changed-upstream-key".into();
    local
        .configuration()
        .overwrite_codex_provider_file(changed)
        .unwrap();
    assert!(gateway
        .prepare_restored(&local, AppKind::Codex, &saved)
        .is_err());

    let changed = local
        .configuration()
        .list_codex_providers()
        .unwrap()
        .remove(0);
    local
        .configuration()
        .delete_codex_provider(&changed.profile.id, &changed.file_hash)
        .unwrap();
    saved_route(&local, &gateway, AppKind::Codex);
    assert!(gateway
        .prepare_restored(&local, AppKind::Codex, &saved)
        .is_err());
    gateway.shutdown();
}

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
            AppKind::Codex => {
                let token = archived
                    .split("asb_codex_")
                    .nth(1)
                    .and_then(|tail| tail.split('/').next())
                    .expect("Codex route token");
                archived.replace(token, &"0".repeat(token.len()))
            }
            AppKind::Claude => {
                let token = archived
                    .split("asb_local_")
                    .nth(1)
                    .and_then(|tail| tail.split('/').next())
                    .expect("Claude route token");
                archived.replace(token, &"0".repeat(token.len()))
            }
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
fn damaged_configuration_does_not_panic_during_gateway_observation() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    for app in [AppKind::Codex, AppKind::Claude] {
        let target = local.target(app).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, "{ invalid config [").unwrap();
    }
    let gateway = GatewayController::start(&local);
    assert_eq!(gateway.observe(&local).status, GatewayStatusKind::Standby);
    gateway.shutdown();
}
