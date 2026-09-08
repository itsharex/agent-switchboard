use super::*;
use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

fn connect(url: &str) -> WebSocket<TcpStream> {
    let url = url.replace("http://", "ws://");
    let mut request = url.clone().into_client_request().unwrap();
    request.headers_mut().insert(
        "Authorization",
        "Bearer fixture-official-access".parse().unwrap(),
    );
    let parsed = reqwest::Url::parse(&url).unwrap();
    let tcp = TcpStream::connect(("127.0.0.1", parsed.port().unwrap())).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    tungstenite::client(request, tcp).unwrap().0
}

fn fixture() -> (
    tempfile::TempDir,
    GatewayController,
    crate::gateway::GatewayProjection,
) {
    let dir = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(dir.path().join("state"));
    let profile = sandbox_profile(
        &state,
        AppKind::Codex,
        "idle sockets",
        "http://127.0.0.1:9".into(),
        "fixture-upstream-key".into(),
        UpstreamProtocol::Responses,
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project(&SwitchPlan::direct(
            profile,
            default_client_settings(AppKind::Codex),
        ))
        .unwrap();
    gateway.commit(&projection, || Ok(())).unwrap();
    (dir, gateway, projection)
}

#[test]
fn idle_websockets_do_not_occupy_request_slots_and_close_on_shutdown() {
    let (_dir, gateway, projection) = fixture();
    let mut sockets: Vec<_> = (0..10)
        .map(|_| connect(&codex_endpoint(&projection)))
        .collect();
    assert_eq!(gateway.inner.inflight.load(Ordering::Acquire), 0);
    let response = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap()
        .post(codex_endpoint(&projection))
        .body("not json")
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 422);
    gateway.shutdown();
    let start = Instant::now();
    for socket in &mut sockets {
        assert!(socket.read().is_err());
    }
    assert!(start.elapsed() < Duration::from_secs(3));
}

#[test]
fn stopping_old_listener_closes_idle_socket_without_stopping_new_listener() {
    let (_dir, gateway, projection) = fixture();
    let mut old = connect(&codex_endpoint(&projection));
    let old_listener = gateway.listening().unwrap();
    let next = crate::gateway::BoundListener::bind(0).unwrap();
    gateway.spawn_serve(next.clone()).unwrap();
    let endpoint = codex_endpoint(&projection).replace(
        &format!(":{}", old_listener.port()),
        &format!(":{}", next.port()),
    );
    old_listener.stop();
    assert!(old.read().is_err());
    let mut current = connect(&endpoint);
    current
        .send(Message::Text(
            json!({"type":"response.create","model":"sandbox-model","input":[],"generate":false})
                .to_string(),
        ))
        .unwrap();
    let first: Value = serde_json::from_str(&current.read().unwrap().into_text().unwrap()).unwrap();
    assert_eq!(first["type"], "response.created");
    let completed: Value =
        serde_json::from_str(&current.read().unwrap().into_text().unwrap()).unwrap();
    assert_eq!(completed["type"], "response.completed");
    let deadline = Instant::now() + Duration::from_secs(1);
    while gateway.inner.inflight.load(Ordering::Acquire) != 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(gateway.inner.inflight.load(Ordering::Acquire), 0);
    next.stop();
    gateway.shutdown();
}
