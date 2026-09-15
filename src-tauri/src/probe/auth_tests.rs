//! Authentication boundaries use synthetic credentials and loopback only.

use super::{fetch_models, provider_auth_headers, ProviderModel};
use asb_core::contracts::{UpstreamProtocol, UsageQuery};
use asb_core::AuthenticationScheme;
use reqwest::blocking::RequestBuilder;
use serde_json::{json, Value};
use std::net::TcpListener;
use std::sync::mpsc::{self, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server};

const KEY: &str = "probe-auth-fixture-key";
const PROTOCOLS: [UpstreamProtocol; 3] = [
    UpstreamProtocol::AnthropicMessages,
    UpstreamProtocol::ChatCompletions,
    UpstreamProtocol::Responses,
];
const SELECTIONS: [Option<AuthenticationScheme>; 3] = [
    Some(AuthenticationScheme::Bearer),
    Some(AuthenticationScheme::XApiKey),
    None,
];

fn expected_auth(
    protocol: UpstreamProtocol,
    selected: Option<AuthenticationScheme>,
) -> AuthenticationScheme {
    match selected {
        Some(scheme) => scheme,
        None if protocol == UpstreamProtocol::AnthropicMessages => AuthenticationScheme::XApiKey,
        None => AuthenticationScheme::Bearer,
    }
}

fn header_values<'a>(request: &'a Request, name: &'static str) -> Vec<&'a str> {
    request
        .headers()
        .iter()
        .filter(|header| header.field.equiv(name))
        .map(|header| header.value.as_str())
        .collect()
}

fn accepts_headers(
    request: &Request,
    protocol: UpstreamProtocol,
    expected: AuthenticationScheme,
) -> bool {
    let authorization = header_values(request, "authorization");
    let api_key = header_values(request, "x-api-key");
    let selected_only = match expected {
        asb_core::AuthenticationScheme::XGoogApiKey => {
            panic!("Google native has a separate Claude fixture")
        }
        AuthenticationScheme::Bearer => {
            authorization == [format!("Bearer {KEY}")] && api_key.is_empty()
        }
        AuthenticationScheme::XApiKey => api_key == [KEY] && authorization.is_empty(),
    };
    let version = header_values(request, "anthropic-version");
    selected_only
        && if protocol == UpstreamProtocol::AnthropicMessages {
            version == ["2023-06-01"]
        } else {
            version.is_empty()
        }
}

fn fixture(
    protocol: UpstreamProtocol,
    expected: AuthenticationScheme,
    status: u16,
    body: Value,
) -> (String, JoinHandle<bool>) {
    let server = Server::http(("127.0.0.1", 0)).expect("bind loopback");
    let origin = format!("http://{}", server.server_addr().to_ip().unwrap());
    let base = if protocol == UpstreamProtocol::AnthropicMessages {
        origin
    } else {
        format!("{origin}/v1")
    };
    let worker = thread::spawn(move || {
        let request = server
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .expect("bounded model request");
        let accepted = request.method() == &Method::Get
            && request.url() == "/v1/models"
            && accepts_headers(&request, protocol, expected);
        let response = if accepted {
            body
        } else {
            json!({ "error": { "message": "selected authentication required" } })
        };
        request
            .respond(
                Response::from_string(response.to_string())
                    .with_status_code(if accepted { status } else { 401 })
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
                    .with_header(Header::from_bytes("x-request-id", format!("req-{KEY}")).unwrap()),
            )
            .expect("respond to loopback request");
        accepted
    });
    (base, worker)
}

fn with_auth(request: RequestBuilder, scheme: AuthenticationScheme) -> RequestBuilder {
    match scheme {
        asb_core::AuthenticationScheme::XGoogApiKey => {
            panic!("Google native has a separate Claude fixture")
        }
        AuthenticationScheme::Bearer => request.header("authorization", format!("Bearer {KEY}")),
        AuthenticationScheme::XApiKey => request.header("x-api-key", KEY),
    }
}

fn models_url(base: &str, protocol: UpstreamProtocol) -> String {
    let path = if protocol == UpstreamProtocol::AnthropicMessages {
        "/v1/models"
    } else {
        "/models"
    };
    format!("{base}{path}")
}

fn unexpected_request_fixture() -> (String, Sender<()>, JoinHandle<bool>) {
    let server = Server::http(("127.0.0.1", 0)).unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let (stop, stopped) = mpsc::channel();
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(request) = server.recv_timeout(Duration::from_millis(20)).unwrap() {
                request
                    .respond(
                        Response::from_string("selected authentication required")
                            .with_status_code(401),
                    )
                    .unwrap();
                return true;
            }
            if !matches!(stopped.try_recv(), Err(TryRecvError::Empty)) {
                return false;
            }
            assert!(
                Instant::now() < deadline,
                "bounded invalid-header fixture expired"
            );
        }
    });
    (base, stop, worker)
}

#[test]
fn model_fetch_honors_both_auth_schemes_and_protocol_defaults() {
    for protocol in PROTOCOLS {
        for selected in SELECTIONS {
            let (base, worker) = fixture(
                protocol,
                expected_auth(protocol, selected),
                200,
                json!({ "data": [{ "id": "fixture-model", "owned_by": "fixture-vendor" }] }),
            );
            let result = fetch_models(&base, KEY, protocol, selected, &Default::default());
            assert!(
                worker.join().expect("loopback worker"),
                "auth or version header mismatch"
            );
            assert_eq!(
                result.unwrap(),
                vec![ProviderModel {
                    id: "fixture-model".to_string(),
                    owned_by: Some("fixture-vendor".to_string()),
                }]
            );
        }
    }
}

#[test]
fn model_fixture_rejects_wrong_and_dual_authentication() {
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    for protocol in PROTOCOLS {
        for expected in [AuthenticationScheme::Bearer, AuthenticationScheme::XApiKey] {
            for dual in [false, true] {
                let other = match expected {
                    asb_core::AuthenticationScheme::XGoogApiKey => {
                        panic!("Google native has a separate Claude fixture")
                    }
                    AuthenticationScheme::Bearer => AuthenticationScheme::XApiKey,
                    AuthenticationScheme::XApiKey => AuthenticationScheme::Bearer,
                };
                let (base, worker) = fixture(protocol, expected, 200, json!({ "data": [] }));
                let mut request = with_auth(client.get(models_url(&base, protocol)), other);
                if dual {
                    request = with_auth(request, expected);
                }
                if protocol == UpstreamProtocol::AnthropicMessages {
                    request = request.header("anthropic-version", "2023-06-01");
                }
                let response = request.send();
                assert!(
                    !worker.join().unwrap(),
                    "fixture accepted an unselected auth header"
                );
                assert_eq!(response.unwrap().status().as_u16(), 401);
            }
        }
    }
}

#[test]
fn usage_header_strings_keep_auth_and_anthropic_version_independent() {
    for protocol in PROTOCOLS {
        for selected in SELECTIONS {
            let mut expected = match expected_auth(protocol, selected) {
                asb_core::AuthenticationScheme::XGoogApiKey => {
                    panic!("Google native has a separate Claude fixture")
                }
                AuthenticationScheme::Bearer => format!("Authorization: Bearer {KEY}"),
                AuthenticationScheme::XApiKey => format!("x-api-key: {KEY}"),
            };
            if protocol == UpstreamProtocol::AnthropicMessages {
                expected.push_str("\r\nanthropic-version: 2023-06-01");
            }
            assert_eq!(provider_auth_headers(KEY, protocol, selected), Ok(expected));
        }
    }
}

#[test]
fn model_fetch_redacts_credentials_from_error_bodies_and_request_ids() {
    for protocol in PROTOCOLS {
        for selected in SELECTIONS {
            let (base, worker) = fixture(
                protocol,
                expected_auth(protocol, selected),
                401,
                json!({ "error": { "message": format!("rejected {KEY}") } }),
            );
            let result = fetch_models(&base, KEY, protocol, selected, &Default::default());
            assert!(worker.join().unwrap(), "fixture rejected request headers");
            let error = result.expect_err("upstream rejected fixture credential");
            assert!(
                !error.contains(KEY),
                "model-fetch error exposed the credential"
            );
            assert!(error.contains("HTTP 401"));
            assert!(error.contains(&models_url(&base, protocol)));
        }
    }
}

#[test]
fn invalid_model_auth_headers_fail_before_network_without_exposing_key() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    for protocol in PROTOCOLS {
        for selected in SELECTIONS {
            for suffix in [
                "\r\nx-injected: fixture",
                "\nfixture",
                "\0fixture",
                "\u{7f}fixture",
            ] {
                let key = format!("{KEY}{suffix}");
                let result = fetch_models(&base, &key, protocol, selected, &Default::default());
                let error = result.expect_err("invalid header must fail");
                assert!(
                    !error.contains(KEY),
                    "invalid-header diagnostic exposed key text"
                );
                assert!(!error.contains("x-injected"));
                assert_eq!(
                    listener.accept().unwrap_err().kind(),
                    std::io::ErrorKind::WouldBlock
                );
            }
        }
    }
}

#[test]
fn declarative_usage_rejects_crlf_keys_before_creating_another_auth_header() {
    for protocol in PROTOCOLS {
        for selected in SELECTIONS {
            let (base, stop, worker) = unexpected_request_fixture();
            let other = match expected_auth(protocol, selected) {
                asb_core::AuthenticationScheme::XGoogApiKey => {
                    panic!("Google native has a separate Claude fixture")
                }
                AuthenticationScheme::Bearer => "x-api-key",
                AuthenticationScheme::XApiKey => "authorization",
            };
            let key = format!("{KEY}\r\n{other}: injected-fixture-value");
            let query = UsageQuery::Declarative {
                url: format!("{base}/usage"),
                remaining_path: Some("/remaining".into()),
                used_path: None,
                total_path: None,
                unit: None,
                refresh_interval_minutes: 0,
            };
            let result =
                crate::usage_query::run_usage_query(&query, &key, None, protocol, selected);
            let _ = stop.send(());
            let contacted = worker.join().unwrap();
            assert!(
                !contacted,
                "CRLF credential reached the network as separate header lines"
            );
            let error = result.expect_err("invalid usage-query credential must fail locally");
            assert!(!error.contains(KEY));
            assert!(!error.contains("injected-fixture-value"));
        }
    }
}
