//! Isolated loopback tests: real sockets, no external network, no user state.

use super::*;

/// Snapshot and environment state are process-wide; stateful tests serialize.
static STATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn manual(url: &str) -> OutboundProxySettings {
    OutboundProxySettings {
        mode: OutboundProxyMode::Manual,
        url: Some(url.into()),
        ..OutboundProxySettings::default()
    }
}

fn activate(settings: &OutboundProxySettings) {
    store_snapshot(Arc::new(Snapshot {
        revision: settings.revision.clone(),
        mode: settings.mode,
        url: settings.url.clone(),
    }));
}

fn blocking_client() -> reqwest::blocking::Client {
    apply_blocking(reqwest::blocking::ClientBuilder::new())
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap()
}

#[test]
fn settings_validate_scheme_presence_and_masks_stay_secret_free() {
    for valid in [
        "http://127.0.0.1:7890",
        "https://proxy.example.test:8443",
        "socks5://127.0.0.1:1080",
        "socks5h://127.0.0.1:1080",
        "http://user:secret@127.0.0.1:7890",
    ] {
        assert!(manual(valid).validate().is_ok(), "{valid}");
    }
    for invalid in ["ftp://127.0.0.1:21", "not a url", "http://", ""] {
        assert!(manual(invalid).validate().is_err(), "{invalid}");
    }
    let mut missing = manual("http://127.0.0.1:7890");
    missing.url = None;
    assert!(missing.validate().is_err());
    let mut direct_with_url = OutboundProxySettings::default();
    direct_with_url.url = Some("http://127.0.0.1:7890".into());
    assert!(direct_with_url.validate().is_err());
    let mut system_with_url = OutboundProxySettings {
        mode: OutboundProxyMode::System,
        ..OutboundProxySettings::default()
    };
    system_with_url.url = Some("http://127.0.0.1:7890".into());
    assert!(system_with_url.validate().is_err());
    assert_eq!(
        mask_url("http://user:secret@127.0.0.1:7890"),
        "http://***@127.0.0.1:7890"
    );
    assert_eq!(
        mask_url("socks5://127.0.0.1:1080"),
        "socks5://127.0.0.1:1080"
    );
}

#[test]
fn settings_round_trip_with_compare_and_swap() {
    let _guard = STATE.lock().unwrap_or_else(|poison| poison.into_inner());
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let initial = read(root).unwrap();
    assert_eq!(initial.mode, OutboundProxyMode::None);
    let saved = save(root, manual("socks5://127.0.0.1:1080"), &initial.revision).unwrap();
    assert_eq!(read(root).unwrap(), saved);
    assert_ne!(saved.revision, initial.revision);
    assert!(save(root, manual("http://127.0.0.1:1"), &initial.revision).is_err());
    std::fs::write(
        root.join("outbound-proxy.json"),
        "{\"version\":1,\"mode\":\"maybe\",\"revision\":\"x\"}",
    )
    .unwrap();
    assert!(read(root).is_err());
}

#[test]
fn load_publishes_persisted_settings_to_the_runtime_snapshot() {
    let _guard = STATE.lock().unwrap_or_else(|poison| poison.into_inner());
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let initial = read(root).unwrap();
    let saved = save(root, manual("http://127.0.0.1:9"), &initial.revision).unwrap();
    load(root).unwrap();
    assert_eq!(
        decision_for(&snapshot()),
        EgressDecision::Proxy("http://127.0.0.1:9".into())
    );
    let _ = saved;
    // Back to defaults for the other tests in this binary.
    let direct = OutboundProxySettings::default();
    activate(&direct);
}

#[test]
fn manual_mode_routes_http_requests_through_the_proxy() {
    let _guard = STATE.lock().unwrap_or_else(|poison| poison.into_inner());
    // An HTTP proxy receives the absolute-form request line; the origin form
    // proves direct egress. Port 9 (discard) is never the real target.
    let (proxy_address, proxy) = serve_once();
    activate(&manual(&format!("http://{proxy_address}")));
    let client = blocking_client();
    let response = client.get("http://127.0.0.1:9/health").send().unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let head = proxy.join().unwrap();
    assert!(
        head.starts_with("GET http://127.0.0.1:9/health"),
        "request did not use absolute-form through the proxy: {head}"
    );
    activate(&OutboundProxySettings::default());
}

#[test]
fn none_mode_goes_direct_even_after_a_manual_snapshot() {
    let _guard = STATE.lock().unwrap_or_else(|poison| poison.into_inner());
    let (origin, server) = serve_once();
    activate(&OutboundProxySettings::default());
    let client = blocking_client();
    let response = client
        .get(format!("http://{origin}/health"))
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let head = server.join().unwrap();
    assert!(
        head.starts_with("GET /health"),
        "origin-form expected: {head}"
    );
}

#[test]
fn system_mode_refuses_to_loop_through_the_gateway_itself() {
    let _guard = STATE.lock().unwrap_or_else(|poison| poison.into_inner());
    let (origin, server) = serve_once();
    // Pretend this listener is the gateway: env proxies pointing at our own
    // port must degrade to direct egress.
    note_gateway_port(origin.port());
    let previous = std::env::var("http_proxy").ok();
    std::env::set_var("http_proxy", format!("http://127.0.0.1:{}", origin.port()));
    let result = std::thread::spawn(|| {
        let settings = OutboundProxySettings {
            mode: OutboundProxyMode::System,
            ..OutboundProxySettings::default()
        };
        activate(&settings);
        blocking_client()
    })
    .join()
    .unwrap();
    match previous {
        Some(value) => std::env::set_var("http_proxy", value),
        None => std::env::remove_var("http_proxy"),
    }
    note_gateway_port(0);
    // The "gateway" listener doubles as the request target: a looped request
    // would arrive as absolute-form, direct egress arrives as origin-form.
    let response = result
        .get(format!("http://{origin}/health"))
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let head = server.join().unwrap();
    assert!(
        head.starts_with("GET /health"),
        "expected direct egress: {head}"
    );
    activate(&OutboundProxySettings::default());
}

#[test]
fn cached_clients_rebuild_only_when_the_revision_moves() {
    let _guard = STATE.lock().unwrap_or_else(|poison| poison.into_inner());
    activate(&OutboundProxySettings::default());
    let builds = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = builds.clone();
    let _first = cached_client("cache-test", move |builder| {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        builder.build().unwrap()
    });
    let counter = builds.clone();
    let _second = cached_client("cache-test", move |builder| {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        builder.build().unwrap()
    });
    assert_eq!(
        builds.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "same revision must reuse the cached client"
    );
    store_snapshot(Arc::new(Snapshot {
        revision: uuid::Uuid::new_v4().to_string(),
        mode: OutboundProxyMode::None,
        url: None,
    }));
    let counter = builds.clone();
    let _third = cached_client("cache-test", move |builder| {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        builder.build().unwrap()
    });
    assert_eq!(
        builds.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "a new revision must rebuild the client"
    );
}

/// Accepts one connection, reads the request head, answers 200 with "ok".
fn serve_once() -> (std::net::SocketAddr, std::thread::JoinHandle<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0_u8; 4096];
        let read = stream.read(&mut buffer).unwrap();
        let head = String::from_utf8_lossy(&buffer[..read]).into_owned();
        let response = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok";
        stream.write_all(response.as_bytes()).unwrap();
        head
    });
    (address, handle)
}
