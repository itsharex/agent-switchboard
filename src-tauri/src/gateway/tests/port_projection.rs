use super::*;

/// The confirmed port change rewrites every owning client configuration in
/// one transaction, keeps the capability token stable, moves the persisted
/// port, swaps the live listener, and cleans the journal.
#[test]
fn confirmed_port_change_rewrites_clients_and_keeps_tokens_stable() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let gateway = GatewayController::start(&state);
    let from_port = gateway.configured_port();

    let record = state
        .configuration()
        .create_provider(claude_gateway_draft("upstream-key"))
        .unwrap();
    let projection = gateway.project(&plan(record.profile.clone())).unwrap();
    let target = state.target(AppKind::Claude).unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(
        &target,
        asb_core::adapter::render("{}", &projection.plan).unwrap(),
    )
    .unwrap();
    gateway.commit(&projection, || Ok(())).unwrap();
    let original_token = projection.plan.client_api_key().to_string();

    let preparations = PortChangePreparations::default();
    let port_plan = prepare_available_port(&gateway, &state, &preparations);
    let to_port = port_plan.to_port;
    assert_eq!(port_plan.from_port, from_port);
    assert_eq!(port_plan.to_port, to_port);
    assert_eq!(port_plan.clients.len(), 1);
    assert_eq!(
        port_plan.clients[0].current_base_url,
        format!("http://127.0.0.1:{from_port}")
    );
    assert_eq!(
        port_plan.clients[0].new_base_url,
        format!("http://127.0.0.1:{to_port}")
    );

    port_change::commit(&gateway, &state, &preparations, &port_plan.preparation_id).unwrap();

    let text = fs::read_to_string(&target).unwrap();
    assert!(text.contains(&format!("http://127.0.0.1:{to_port}")));
    assert!(text.contains(&original_token), "capability token is stable");
    assert!(
        !text.contains("upstream-key"),
        "upstream secret never lands"
    );

    let saved: GatewayStateFile =
        serde_json::from_str(&fs::read_to_string(state.gateway_state_path()).unwrap()).unwrap();
    assert_eq!(saved.port, to_port);
    assert!(saved.claude_route.is_some());
    assert!(!state
        .gateway_state_path()
        .with_file_name("gateway-port-journal.json")
        .exists());
    assert!(gateway.is_listening());
    assert!(gateway.has_active_routes());
    assert_eq!(
        state
            .configuration()
            .latest_config_write(AppKind::Claude)
            .unwrap()
            .unwrap()
            .operation,
        WriteOperation::GatewayPortChange
    );
    let observation = gateway.observe(&state);
    assert_eq!(observation.status, GatewayStatusKind::Running);
    assert_eq!(observation.listening_port, Some(to_port));
    gateway.shutdown();
}

fn prepare_available_port(
    gateway: &GatewayController,
    state: &LocalState,
    preparations: &PortChangePreparations,
) -> port_change::GatewayPortChangePlan {
    // A probe port can be re-stolen by the OS between release and re-bind on
    // busy hosts; retry with fresh probe ports until one is bindable.
    let mut port_plan = None;
    for _ in 0..10 {
        let probe = Server::http(("127.0.0.1", 0)).unwrap();
        let to_port = probe.server_addr().to_ip().unwrap().port();
        drop(probe);
        match port_change::prepare(&gateway, &state, &preparations, to_port) {
            Ok(plan) => {
                port_plan = Some(plan);
                break;
            }
            Err(error) if error.contains("端口已被占用") => continue,
            Err(error) => panic!("{error}"),
        }
    }
    port_plan.expect("an eventually bindable probe port")
}
