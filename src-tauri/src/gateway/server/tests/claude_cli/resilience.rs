//! The rest of the real-CLI resilience matrix: hard-kill crash recovery,
//! port-conflict detection and repair, and a mid-session provider hot switch.
//! Isolated files, fake credentials, loopback.

use super::recovery::{anthropic_text_stream, apply_projection, stdout_of, StopGatewayLike};
use super::*;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn streaming_upstream(
    server: Server,
    text: &'static str,
) -> (
    mpsc::Receiver<String>,
    mpsc::Sender<()>,
    thread::JoinHandle<()>,
) {
    let (request_sender, request_receiver) = mpsc::channel();
    let (stop, stopped) = mpsc::channel();
    let worker = thread::spawn(move || loop {
        match stopped.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        let Some(mut request) = server.recv_timeout(Duration::from_millis(100)).unwrap() else {
            continue;
        };
        let mut body = String::new();
        request.as_reader().read_to_string(&mut body).unwrap();
        let _ = request_sender.send(body);
        let _ = request.respond(
            Response::from_data(anthropic_text_stream(text).into_bytes()).with_header(
                crate::gateway::server::tests::content_type("text/event-stream"),
            ),
        );
    });
    (request_receiver, stop, worker)
}

fn received_count(receiver: &mpsc::Receiver<String>) -> usize {
    let mut total = 0;
    while receiver.try_recv().is_ok() {
        total += 1;
    }
    total
}

/// A killed listener socket is released asynchronously by its serve thread;
/// wait until the port refuses connections before anything rebinds it.
fn wait_until_unbound(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_err() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("the killed gateway port {port} was never released");
}

struct Sandbox {
    local: LocalState,
    profile: asb_core::ProviderProfile,
}

fn takeover_sandbox(directory: &Path, name: &str, upstream_url: &str, key: &str) -> Sandbox {
    let local = LocalState::from_root(directory.join("state"));
    crate::gateway::failover::save(
        local.root(),
        &crate::gateway::failover::ClaudeFailoverPolicy {
            takeover: true,
            ..Default::default()
        },
    )
    .unwrap();
    let profile = sandbox_profile(
        &local,
        AppKind::Claude,
        name,
        upstream_url.to_string(),
        key.to_string(),
        UpstreamProtocol::AnthropicMessages,
    );
    Sandbox { local, profile }
}

struct TakenOver {
    config: PathBuf,
    work: PathBuf,
    home: PathBuf,
    settings: PathBuf,
}

fn apply_takeover(
    directory: &Path,
    gateway: &GatewayController,
    local: &LocalState,
    profile: &asb_core::ProviderProfile,
) -> TakenOver {
    let plan = SwitchPlan::direct(profile.clone(), default_client_settings(AppKind::Claude));
    let projection = gateway.project_with_candidates(local, &plan).unwrap();
    // The client file must live exactly where the application resolves it
    // (`local.target`), so a relaunched controller rehydrates the persisted
    // route against the very file the isolated CLI reads.
    let settings = local.target(AppKind::Claude).unwrap();
    let config = settings.parent().unwrap().to_path_buf();
    for path in [
        &config,
        &directory.join("work"),
        &directory.join("home"),
        &directory.join("backups"),
    ] {
        fs::create_dir_all(path).unwrap();
    }
    apply_projection(&settings, &projection, gateway, directory);
    TakenOver {
        config,
        work: directory.join("work"),
        home: directory.join("home"),
        settings,
    }
}

fn run_cli(taken: &TakenOver, instruction: &str, upstream_url: &str) -> std::process::Output {
    runner::run_with_model(
        &taken.config,
        &taken.work,
        &taken.home,
        instruction,
        "Read",
        upstream_url,
        Some("claude-opus-4-6"),
    )
}

/// A short-deadline run for situations where the client is expected to be
/// unable to complete: a timeout is killed and returned, not fatal.
fn run_cli_until_deadline(
    taken: &TakenOver,
    instruction: &str,
    upstream_url: &str,
    deadline_secs: u64,
) -> std::process::Output {
    runner::run_with_deadline(
        &taken.config,
        &taken.work,
        &taken.home,
        instruction,
        "Read",
        upstream_url,
        Some("claude-opus-4-6"),
        deadline_secs,
        false,
    )
}

#[test]
#[ignore = "requires the pinned Claude Code CLI; only temporary files, fake credentials and loopback are used"]
fn actual_claude_recovers_when_the_gateway_process_dies_under_a_taken_over_client() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let upstream_url = endpoint(&upstream);
    let key = format!("crash-{}", uuid::Uuid::new_v4());
    let (requests, stop, worker) = streaming_upstream(upstream, "crash fixture ok");
    let sandbox = takeover_sandbox(directory.path(), "CLI 崩溃恢复", &upstream_url, &key);
    let gateway = GatewayController::start(&sandbox.local);
    let taken = apply_takeover(directory.path(), &gateway, &sandbox.local, &sandbox.profile);
    let base_url = gateway.configured_base_url();
    let untouched = fs::read_to_string(&taken.settings).unwrap();
    assert!(untouched.contains(&base_url) && !untouched.contains(&key));
    let mut served = 0;

    let first = run_cli(&taken, "Return exactly: crash one", &upstream_url);
    assert!(
        first.status.success() && stdout_of(&first).contains("crash fixture ok"),
        "the CLI must complete through the live gateway; stdout={:?}; stderr={:?}",
        stdout_of(&first),
        String::from_utf8_lossy(&first.stderr)
    );
    served += received_count(&requests);
    assert_eq!(served, 1);

    // Hard kill: the socket dies like a crashed process. No graceful exit,
    // no client rewrite — the file still points at a dead loopback address.
    let port = gateway.configured_port();
    gateway.listening().expect("live listener").stop();
    wait_until_unbound(port);
    assert!(!gateway.is_listening());
    let during = run_cli_until_deadline(&taken, "Return exactly: crash two", &upstream_url, 15);
    assert!(
        !during.status.success() && !stdout_of(&during).contains("crash fixture ok"),
        "the takeover-bound CLI must fail while the gateway is dead; stdout={:?}; stderr={:?}",
        stdout_of(&during),
        String::from_utf8_lossy(&during.stderr)
    );
    served += received_count(&requests);
    assert_eq!(served, 1, "the dead route must have served nothing");

    // Relaunch: a fresh controller over the same state root recovers the
    // persisted route against the untouched client file and serves again.
    let revived = GatewayController::start(&sandbox.local);
    let third;
    {
        let _stop_revived = StopGatewayLike(revived.clone());
        assert!(revived.is_listening());
        assert_eq!(revived.configured_base_url(), base_url);
        let observation = revived.observe(&sandbox.local);
        assert_eq!(
            observation.status,
            crate::gateway::GatewayStatusKind::Running
        );
        assert!(
            observation.routes.iter().any(
                |route| route.app == AppKind::Claude && route.profile_id == sandbox.profile.id
            ),
            "the crash must not strand the takeover route"
        );
        assert_eq!(fs::read_to_string(&taken.settings).unwrap(), untouched);
        third = run_cli(&taken, "Return exactly: crash three", &upstream_url);
    }
    assert!(
        third.status.success() && stdout_of(&third).contains("crash fixture ok"),
        "the CLI must complete after the crash recovery; stdout={:?}; stderr={:?}",
        stdout_of(&third),
        String::from_utf8_lossy(&third.stderr)
    );
    served += received_count(&requests);
    assert_eq!(served, 2);
    assert!(!stdout_of(&third).contains(&key));
    let _ = stop.send(());
    worker.join().unwrap();
}

#[test]
#[ignore = "requires the pinned Claude Code CLI; only temporary files, fake credentials and loopback are used"]
fn actual_claude_reports_and_repairs_a_conflicted_gateway_port_for_real_clients() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let upstream_url = endpoint(&upstream);
    let key = format!("conflict-{}", uuid::Uuid::new_v4());
    let (requests, stop, worker) = streaming_upstream(upstream, "conflict fixture ok");
    let sandbox = takeover_sandbox(directory.path(), "CLI 端口冲突", &upstream_url, &key);
    let gateway = GatewayController::start(&sandbox.local);
    let taken = apply_takeover(directory.path(), &gateway, &sandbox.local, &sandbox.profile);
    let base_url = gateway.configured_base_url();
    let port = gateway.configured_port();
    let first = run_cli(&taken, "Return exactly: conflict one", &upstream_url);
    assert!(
        first.status.success() && stdout_of(&first).contains("conflict fixture ok"),
        "the CLI must complete before the conflict; stdout={:?}; stderr={:?}",
        stdout_of(&first),
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(received_count(&requests), 1);
    let untouched = fs::read_to_string(&taken.settings).unwrap();

    // An unrelated process takes the saved gateway address while the
    // listener is down, so the relaunch cannot bind.
    gateway.listening().expect("live listener").stop();
    wait_until_unbound(port);
    let squatter = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
    let conflicted = GatewayController::start(&sandbox.local);
    let second;
    {
        let _stop_conflicted = StopGatewayLike(conflicted.clone());
        assert!(!conflicted.is_listening());
        assert_eq!(conflicted.configured_port(), port);
        let observation = conflicted.observe(&sandbox.local);
        assert_eq!(
            observation.status,
            crate::gateway::GatewayStatusKind::PortConflict
        );
        let failure = observation.failure.expect("a holder-identifying report");
        assert_eq!(failure.kind, crate::gateway::GatewayFailureKind::PortInUse);
        assert_eq!(failure.port, port);
        assert_eq!(failure.process.expect("holder").pid, std::process::id());
        let plan = SwitchPlan::direct(
            sandbox.profile.clone(),
            default_client_settings(AppKind::Claude),
        );
        assert!(
            conflicted
                .project_with_candidates(&sandbox.local, &plan)
                .is_err(),
            "no gateway-dependent client write may land while nothing serves the address"
        );

        // The holder exits; the repair entry rebinds the same saved address
        // and the CLI completes again, with the client file never rewritten.
        drop(squatter);
        let repaired = conflicted.retry_bind(&sandbox.local);
        assert_eq!(repaired.status, crate::gateway::GatewayStatusKind::Running);
        assert_eq!(conflicted.listening_base_url(), Some(base_url.clone()));
        assert_eq!(fs::read_to_string(&taken.settings).unwrap(), untouched);
        second = run_cli(&taken, "Return exactly: conflict two", &upstream_url);
    }
    assert!(
        second.status.success() && stdout_of(&second).contains("conflict fixture ok"),
        "the CLI must complete after the port repair; stdout={:?}; stderr={:?}",
        stdout_of(&second),
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(!stdout_of(&second).contains(&key));
    let _ = stop.send(());
    assert_eq!(received_count(&requests) + 1, 2);
    worker.join().unwrap();
}

#[test]
#[ignore = "requires the pinned Claude Code CLI; only temporary files, fake credentials and loopback are used"]
fn actual_claude_hot_switch_moves_the_served_provider_while_the_client_stays_taken_over() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let alpha = Server::http(("127.0.0.1", 0)).unwrap();
    let beta = Server::http(("127.0.0.1", 0)).unwrap();
    let alpha_url = endpoint(&alpha);
    let beta_url = endpoint(&beta);
    let alpha_key = format!("alpha-{}", uuid::Uuid::new_v4());
    let beta_key = format!("beta-{}", uuid::Uuid::new_v4());
    let (alpha_requests, alpha_stop, alpha_worker) =
        streaming_upstream(alpha, "hot switch alpha ok");
    let (beta_requests, beta_stop, beta_worker) = streaming_upstream(beta, "hot switch beta ok");
    let local = LocalState::from_root(directory.path().join("state"));
    crate::gateway::failover::save(
        local.root(),
        &crate::gateway::failover::ClaudeFailoverPolicy {
            takeover: true,
            ..Default::default()
        },
    )
    .unwrap();
    let alpha_profile = sandbox_profile(
        &local,
        AppKind::Claude,
        "CLI 热切换 A",
        alpha_url.clone(),
        alpha_key.clone(),
        UpstreamProtocol::AnthropicMessages,
    );
    let beta_profile = sandbox_profile(
        &local,
        AppKind::Claude,
        "CLI 热切换 B",
        beta_url.clone(),
        beta_key.clone(),
        UpstreamProtocol::AnthropicMessages,
    );
    let gateway = GatewayController::start(&local);
    let _stop_gateway = StopGatewayLike(gateway.clone());
    let taken = apply_takeover(directory.path(), &gateway, &local, &alpha_profile);
    let first = run_cli(&taken, "Return exactly: alpha", &alpha_url);
    assert!(
        first.status.success() && stdout_of(&first).contains("hot switch alpha ok"),
        "the first run must complete on the initial provider; stdout={:?}; stderr={:?}",
        stdout_of(&first),
        String::from_utf8_lossy(&first.stderr)
    );

    // Mid-session switch: the same live controller and the same running CLI
    // installation, only the route and its revision move.
    let beta_plan = SwitchPlan::direct(
        beta_profile.clone(),
        default_client_settings(AppKind::Claude),
    );
    let beta_projection = gateway.project_with_candidates(&local, &beta_plan).unwrap();
    apply_projection(
        &taken.settings,
        &beta_projection,
        &gateway,
        directory.path(),
    );
    let rendered = fs::read_to_string(&taken.settings).unwrap();
    assert!(rendered.contains(&gateway.configured_base_url()));
    assert!(
        !rendered.contains(&alpha_key) && !rendered.contains(&beta_key),
        "neither upstream credential may reach the client file under takeover"
    );
    let second = run_cli(&taken, "Return exactly: beta", &alpha_url);
    assert!(
        second.status.success() && stdout_of(&second).contains("hot switch beta ok"),
        "the switched run must complete on the new provider; stdout={:?}; stderr={:?}",
        stdout_of(&second),
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(!stdout_of(&second).contains(&beta_key));
    let _ = alpha_stop.send(());
    let _ = beta_stop.send(());
    assert_eq!(
        received_count(&alpha_requests),
        1,
        "the first provider must serve exactly the pre-switch turn"
    );
    assert_eq!(
        received_count(&beta_requests),
        1,
        "the second provider must serve exactly the post-switch turn"
    );
    alpha_worker.join().unwrap();
    beta_worker.join().unwrap();
}
