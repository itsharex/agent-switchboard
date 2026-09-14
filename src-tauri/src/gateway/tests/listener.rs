use super::*;

#[test]
fn invalid_gateway_state_is_left_untouched_until_explicit_repair() {
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let path = state.gateway_state_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = b"{not-json";
    fs::write(&path, original).unwrap();

    let gateway = GatewayController::start(&state);
    assert!(!gateway.has_active_routes());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(!fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with("gateway.invalid.")));
    let observation = gateway.observe(&state);
    assert_eq!(observation.status, GatewayStatusKind::NeedsRepair);
    assert!(observation.repair_reason.is_some());
    gateway.shutdown();
}

#[test]
fn legacy_active_map_is_left_untouched_and_requires_reapply() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let path = state.gateway_state_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = r#"{
          "version": 1,
          "identity": "legacy-installation",
          "port": 47821,
          "active": {
            "codex": {"profileId": "legacy-profile", "fingerprint": "legacy-revision"}
          }
        }"#;
    fs::write(&path, original).unwrap();
    let target = state.target(AppKind::Codex).unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(
        target,
        format!(
            "model_provider = 'openai'\nopenai_base_url = 'http://127.0.0.1:47821/codex/asb_codex_{}/v1'\n",
            "a".repeat(64)
        ),
    )
    .unwrap();

    let gateway = GatewayController::start(&state);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert!(!gateway.has_active_route_for(AppKind::Codex));
    let observation = gateway.observe(&state);
    assert_eq!(observation.status, GatewayStatusKind::NeedsRepair);
    assert_eq!(
        observation.repair_reason.as_deref(),
        Some("本机协议网关状态不是当前版本；请在网关页重试，保留诊断副本并重建状态后再重新应用供应商")
    );
    assert!(!fs::read_dir(path.parent().unwrap())
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
    let original = fs::read(&path).unwrap();

    let gateway = GatewayController::start(&state);
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(!fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with("gateway.invalid.")));
    assert_eq!(
        gateway.observe(&state).status,
        GatewayStatusKind::NeedsRepair
    );
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

#[test]
fn shutdown_with_a_failed_listener_preserves_dependent_client_files() {
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
    let client_before = fs::read(&target).unwrap();
    let state_before = fs::read(&path).unwrap();
    gateway.shutdown();
    assert!(gateway.inner.stopping.load(Ordering::Acquire));
    assert_eq!(fs::read(&target).unwrap(), client_before);
    assert_eq!(fs::read(&path).unwrap(), state_before);
}

#[test]
fn shutdown_preserves_active_route_and_restart_restores_it() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let file = crate::gateway::server::tests::sandbox_codex_file(
        &local,
        "shutdown recovery",
        "http://127.0.0.1:9".into(),
        "fixture-upstream-key".into(),
        asb_core::contracts::CodexUpstream::Responses,
    );
    let gateway = GatewayController::start(&local);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .unwrap();
    let target = local.target(AppKind::Codex).unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(
        &target,
        asb_core::adapter::render("", &projection.plan).unwrap(),
    )
    .unwrap();
    gateway.commit(&projection, || Ok(())).unwrap();
    assert!(gateway.has_active_route_for(AppKind::Codex));
    let client_before = fs::read(&target).unwrap();
    let state_before = fs::read(local.gateway_state_path()).unwrap();
    let port = gateway.listening().unwrap().port();

    gateway.shutdown();
    assert!(!gateway.is_listening());
    assert!(InflightGuard::try_acquire(gateway.inner.clone()).is_none());
    assert_eq!(fs::read(&target).unwrap(), client_before);
    assert_eq!(fs::read(local.gateway_state_path()).unwrap(), state_before);

    let restarted = restart_controller(&local);
    assert_eq!(restarted.listening().unwrap().port(), port);
    assert!(restarted.has_active_route_for(AppKind::Codex));
    assert_eq!(fs::read(&target).unwrap(), client_before);
    restarted.shutdown();
}
