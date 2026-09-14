use super::*;

fn broken_state(local: &LocalState, bytes: &[u8]) {
    let path = local.gateway_state_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn diagnostic_copies(local: &LocalState) -> Vec<PathBuf> {
    fs::read_dir(local.gateway_state_path().parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("gateway.invalid.")
        })
        .map(|entry| entry.path())
        .collect()
}

#[test]
fn explicit_retry_preserves_invalid_bytes_and_rebuilds_only_gateway_state() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let original = b"{broken\xff";
    broken_state(&local, original);
    let gateway = GatewayController::start(&local);
    assert!(!gateway.is_listening());
    assert_eq!(fs::read(local.gateway_state_path()).unwrap(), original);
    let retried = gateway.retry_bind(&local);
    assert_eq!(retried.status, GatewayStatusKind::Standby);
    assert!(retried.listening_port.is_some());
    assert_eq!(diagnostic_copies(&local).len(), 1);
    assert_eq!(fs::read(&diagnostic_copies(&local)[0]).unwrap(), original);
    let state: GatewayStateFile =
        serde_json::from_slice(&fs::read(local.gateway_state_path()).unwrap()).unwrap();
    assert!(!state.has_routes());
    gateway.retry_bind(&local);
    assert_eq!(diagnostic_copies(&local).len(), 1);
    assert!(!local.target(AppKind::Claude).unwrap().exists());
    assert!(!local.target(AppKind::Codex).unwrap().exists());
    gateway.shutdown();
}

#[test]
fn repaired_gateway_can_reapply_claude_through_the_normal_executor() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    broken_state(&local, b"not json");
    let target = local.target(AppKind::Claude).unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let previous = r#"{"env":{"ANTHROPIC_BASE_URL":"http://127.0.0.1:53005","ANTHROPIC_AUTH_TOKEN":"asb_local_old"},"hooks":{"keep":true}}"#;
    fs::write(&target, previous).unwrap();
    let record = local
        .configuration()
        .create_provider(claude_gateway_draft("fixture-key"))
        .unwrap();
    let gateway = GatewayController::start(&local);
    let requested = plan(record.profile);
    assert!(gateway.project(&requested).is_err());
    assert_eq!(
        gateway.retry_bind(&local).status,
        GatewayStatusKind::NeedsRepair
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), previous);
    let projection = gateway.project(&requested).unwrap();
    let io = asb_switch::FsIo;
    let backups = local.backup_dir();
    let preview =
        asb_switch::read_preview(&io, &target, &projection.plan, &backups.to_string_lossy())
            .unwrap();
    asb_switch::execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &projection.plan,
            backup_dir: &backups,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        |_| gateway.commit(&projection, || Ok(())),
    )
    .unwrap();
    let root: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
    assert_eq!(root["hooks"]["keep"], true);
    assert_eq!(gateway.observe(&local).status, GatewayStatusKind::Running);
    gateway.shutdown();
}

#[test]
fn explicit_repair_preserves_a_valid_saved_port_and_reports_a_real_conflict() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let blocker = Server::http(("127.0.0.1", 0)).unwrap();
    let port = blocker.server_addr().to_ip().unwrap().port();
    broken_state(
        &local,
        serde_json::json!({"version":999,"port":port,"identity":"old"})
            .to_string()
            .as_bytes(),
    );
    let gateway = GatewayController::start(&local);
    let status = gateway.retry_bind(&local);
    assert_eq!(status.configured_port, port);
    assert_eq!(status.status, GatewayStatusKind::PortConflict);
    assert_eq!(diagnostic_copies(&local).len(), 1);
    drop(blocker);
    for _ in 0..40 {
        if gateway.retry_bind(&local).listening_port == Some(port) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(gateway.observe(&local).listening_port, Some(port));
    gateway.shutdown();
}

#[test]
fn retry_does_not_replace_an_unreadable_state_directory() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let path = local.gateway_state_path();
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("keep"), "original data").unwrap();
    let gateway = GatewayController::start(&local);
    assert_eq!(
        gateway.retry_bind(&local).status,
        GatewayStatusKind::NeedsRepair
    );
    assert_eq!(
        fs::read_to_string(path.join("keep")).unwrap(),
        "original data"
    );
    assert!(!gateway.is_listening());
    assert!(diagnostic_copies(&local).is_empty());
    gateway.shutdown();
}
