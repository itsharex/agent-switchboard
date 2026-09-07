use super::models::{models_path_for, parse_models, provider_auth_headers};
use super::transport::{
    classify_kind, failure_message, grade_for, http_request, parse_url, probe_with_client,
    probe_with_retries, FailureKind, ProbeFailure,
};
use super::*;
use crate::probe::transport::ProbeGrade;
use asb_core::contracts::UpstreamProtocol;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

#[test]
fn parses_http_and_https_urls_with_and_without_ports() {
    let https = parse_url("https://api.example.com/v1").expect("https");
    assert_eq!(https.host, "api.example.com");
    assert_eq!(https.port, 443);
    assert_eq!(https.path, "/v1");
    assert!(https.secure);

    let http = parse_url("http://gateway.local:8443").expect("http");
    assert_eq!(http.port, 8443);
    assert_eq!(http.path, "/");
    assert!(!http.secure);

    let v6 = parse_url("https://[::1]:9443/base?query=1").expect("ipv6");
    assert_eq!(v6.host, "[::1]");
    assert_eq!(v6.port, 9443);
    assert_eq!(v6.path, "/base?query=1");
}

#[test]
fn rejects_urls_without_a_host() {
    assert!(parse_url("https:///path").is_none());
    assert!(parse_url("ftp://example.com").is_none());
}

#[test]
fn reachable_latencies_grade_ok_until_the_slow_threshold() {
    assert_eq!(grade_for(0), ProbeGrade::Ok);
    assert_eq!(grade_for(6_000), ProbeGrade::Ok);
    assert_eq!(grade_for(6_001), ProbeGrade::Slow);
    assert_eq!(grade_for(45_000), ProbeGrade::Slow);
}

fn timeout_failure() -> ProbeFailure {
    ProbeFailure {
        kind: FailureKind::Timeout,
        message: failure_message(FailureKind::Timeout).to_string(),
    }
}

fn refused_failure() -> ProbeFailure {
    ProbeFailure {
        kind: FailureKind::Connect,
        message: failure_message(FailureKind::Connect).to_string(),
    }
}

#[test]
fn timeout_failures_get_exactly_one_retry() {
    let mut attempts = 0;
    let outcome = probe_with_retries(|| {
        attempts += 1;
        if attempts == 1 {
            Err(timeout_failure())
        } else {
            Ok(204)
        }
    });
    assert_eq!(outcome, Ok(204));
    assert_eq!(attempts, 2);
}

#[test]
fn immediate_failures_do_not_retry() {
    let mut attempts = 0;
    let outcome = probe_with_retries(|| {
        attempts += 1;
        Err(refused_failure())
    });
    assert!(outcome.is_err());
    assert_eq!(attempts, 1);
}

#[test]
fn a_second_timeout_is_final() {
    let mut attempts = 0;
    let outcome = probe_with_retries(|| {
        attempts += 1;
        Err(timeout_failure())
    });
    assert!(outcome.is_err());
    assert_eq!(attempts, 2);
}

#[test]
fn failure_kinds_map_to_actionable_classes() {
    assert_eq!(failure_message(FailureKind::Timeout), "连接超时");
    assert_eq!(failure_message(FailureKind::Dns), "域名解析失败（DNS）");
    assert_eq!(failure_message(FailureKind::Connect), "连接被拒绝");
    assert_eq!(failure_message(FailureKind::Tls), "TLS 握手失败");
    assert_eq!(failure_message(FailureKind::Other), "网络请求失败");
}

fn chain(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| text.to_string()).collect()
}

#[test]
fn classify_kind_maps_realistic_transport_errors() {
    assert_eq!(
        classify_kind(&chain(&[
            "client error (Connect)",
            "dns error: failed to lookup address information: Name or service not known",
        ])),
        FailureKind::Dns
    );
    assert_eq!(
        classify_kind(&chain(&[
            "client error (Connect)",
            "invalid peer certificate: UnknownIssuer",
        ])),
        FailureKind::Tls
    );
    assert_eq!(
        classify_kind(&chain(&[
            "client error (Connect)",
            "tcp connect error: Connection refused (os error 111)",
        ])),
        FailureKind::Connect
    );
    assert_eq!(
        classify_kind(&chain(&["operation timed out"])),
        FailureKind::Timeout
    );
    assert_eq!(
        classify_kind(&chain(&["something unfamiliar"])),
        FailureKind::Other
    );
}

#[test]
fn unreachable_endpoint_grades_unreachable() {
    // The operating system chooses an unused local port for this test;
    // dropping the listener makes the following connection deterministic
    // without assuming that a well-known port is unused on the machine.
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve loopback port");
    let address = listener.local_addr().expect("reserved loopback address");
    drop(listener);
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(1))
        .build()
        .expect("direct probe client");
    let result =
        probe_with_client(&format!("http://{address}/health"), &client).expect("probe result");
    assert_eq!(result.grade, ProbeGrade::Unreachable, "{result:?}");
    assert_eq!(result.status, None);
    assert_eq!(result.error.as_deref(), Some("连接被拒绝"));
}

#[test]
fn http_request_passes_non_2xx_status_and_body_through() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback server");
    let address = listener.local_addr().expect("loopback address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .expect("set read timeout");
        let mut request = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            let read = stream.read(&mut chunk).expect("read request");
            assert_ne!(read, 0, "client closed before completing the request");
            request.extend_from_slice(&chunk[..read]);
            if request.windows(4).any(|part| part == b"\r\n\r\n") {
                stream
                        .write_all(
                            b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 16\r\n\r\n{\"error\":\"boom\"}",
                        )
                        .expect("write response");
                return;
            }
        }
    });

    let (status, body) = http_request("GET", &format!("http://{address}/usage"), "", &[])
        .expect("request reaches a 500 responder");
    assert_eq!(status, 500);
    assert_eq!(body, "{\"error\":\"boom\"}");
    server.join().expect("join loopback server");
}

#[test]
fn http_request_sends_nonempty_headers_and_body_to_a_loopback_server() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback server");
    let address = listener.local_addr().expect("loopback address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .expect("set read timeout");
        let mut request = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            let read = stream.read(&mut chunk).expect("read request");
            assert_ne!(read, 0, "client closed before completing the request");
            request.extend_from_slice(&chunk[..read]);

            let Some(headers_end) = request.windows(4).position(|part| part == b"\r\n\r\n") else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..headers_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length:")
                        .or_else(|| line.strip_prefix("content-length:"))
                })
                .map(str::trim)
                .map(|value| value.parse::<usize>().expect("numeric content length"))
                .unwrap_or(0);
            if request.len() >= headers_end + 4 + content_length {
                stream
                    .write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\n\r\nok")
                    .expect("write loopback response");
                return request;
            }
        }
    });

    let (status, body) = http_request(
        "POST",
        &format!("http://{address}/quota"),
        "X-ASB-Test: header-value\r\nContent-Type: text/plain\r\n",
        b"ping",
    )
    .expect("request succeeds with explicit headers");

    assert_eq!(status, 201);
    assert_eq!(body, "ok");
    // hyper normalizes header names to lowercase on the wire; header
    // names are case-insensitive per RFC 9110, so compare likewise.
    let request = String::from_utf8(server.join().expect("join loopback server"))
        .expect("UTF-8 loopback request")
        .to_ascii_lowercase();
    assert!(request.starts_with("post /quota http/1.1\r\n"));
    assert!(request.contains("x-asb-test: header-value\r\n"));
    assert!(request.contains("content-type: text/plain\r\n"));
    assert!(request.ends_with("\r\n\r\nping"));
}

#[test]
fn model_fetch_uses_the_shared_transport() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback server");
    let address = listener.local_addr().expect("loopback address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept model request");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .expect("set read timeout");
        let mut request = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            let read = stream.read(&mut chunk).expect("read model request");
            assert_ne!(read, 0, "client closed before completing the request");
            request.extend_from_slice(&chunk[..read]);
            if request.windows(4).any(|part| part == b"\r\n\r\n") {
                stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 44\r\n\r\n{\"data\":[{\"id\":\"model-a\"},{\"id\":\"model-b\"}]}",
                        )
                        .expect("write model response");
                return request;
            }
        }
    });

    let models = fetch_models(
        &format!("http://{address}/api"),
        "test-credential",
        UpstreamProtocol::Responses,
    )
    .expect("fetch models through shared transport");

    assert_eq!(
        models,
        vec![
            ProviderModel {
                id: "model-a".to_string(),
                owned_by: None,
            },
            ProviderModel {
                id: "model-b".to_string(),
                owned_by: None,
            },
        ]
    );
    let request = String::from_utf8(server.join().expect("join loopback server"))
        .expect("UTF-8 loopback request")
        .to_ascii_lowercase();
    assert!(request.starts_with("get /api/v1/models http/1.1\r\n"));
    assert!(request.contains("authorization: bearer test-credential\r\n"));
}

#[test]
fn model_list_paths_follow_the_base_shape() {
    assert_eq!(models_path_for("https://relay.example"), "/v1/models");
    assert_eq!(models_path_for("https://relay.example/"), "/v1/models");
    assert_eq!(models_path_for("https://relay.example/v1"), "/v1/models");
    assert_eq!(models_path_for("https://relay.example/v2/"), "/v2/models");
    assert_eq!(
        models_path_for("https://relay.example/api/v3"),
        "/api/v3/models"
    );
}

#[test]
fn provider_auth_headers_follow_the_selected_protocol() {
    let headers = provider_auth_headers("sk-test", UpstreamProtocol::Responses);
    assert!(headers.contains("Authorization: Bearer sk-test"));
    assert!(!headers.contains("x-api-key"));
    assert!(!headers.contains("anthropic-version"));

    let headers = provider_auth_headers("sk-test", UpstreamProtocol::AnthropicMessages);
    assert!(!headers.contains("Authorization"));
    assert!(headers.contains("x-api-key: sk-test"));
    assert!(headers.contains("anthropic-version: 2023-06-01"));
}

#[test]
fn models_parse_in_order_and_dedupe_with_vendor() {
    let models = parse_models(
        r#"{"object":"list","data":[
                {"id":"gpt-5.2","owned_by":"openai"},
                {"id":"gpt-5.2","owned_by":"duplicate-vendor"},
                {"id":"claude-x"},
                {"id":"blank","owned_by":"  "},
                {"id":"typed","owned_by":7},
                {}
            ]}"#,
    )
    .expect("models");
    assert_eq!(
        models,
        vec![
            ProviderModel {
                id: "gpt-5.2".to_string(),
                owned_by: Some("openai".to_string()),
            },
            ProviderModel {
                id: "claude-x".to_string(),
                owned_by: None,
            },
            ProviderModel {
                id: "blank".to_string(),
                owned_by: None,
            },
            ProviderModel {
                id: "typed".to_string(),
                owned_by: None,
            },
        ]
    );
    assert!(parse_models("[]").is_err());
    assert!(parse_models("{}").is_err());
}
