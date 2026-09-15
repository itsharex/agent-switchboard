//! Real-request authentication is independent from its wire protocol.

use super::*;
use crate::provider_request::tests::{direct_client, record, success_body, TEST_KEY};
use crate::provider_request::ProviderRequestConnection;
use reqwest::header::HeaderMap;
use std::net::TcpListener;
use std::thread::{self, JoinHandle};
use tiny_http::{Header, Method, Request, Response, Server};

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

fn connection(
    protocol: UpstreamProtocol,
    selected: Option<AuthenticationScheme>,
) -> ProviderRequestConnection {
    let mut profile = record(protocol).profile;
    profile.authentication = selected;
    let connection = ProviderRequestConnection::try_from(&profile).unwrap();
    assert_eq!(connection.authentication, selected);
    connection
}

fn endpoint_path(protocol: UpstreamProtocol) -> &'static str {
    match protocol {
        asb_core::UpstreamProtocol::GeminiGenerateContent => {
            panic!("Google native has a separate Claude fixture")
        }
        UpstreamProtocol::AnthropicMessages => "/v1/messages",
        UpstreamProtocol::ChatCompletions => "/v1/chat/completions",
        UpstreamProtocol::Responses => "/v1/responses",
    }
}

fn assert_built_headers(
    headers: &HeaderMap,
    protocol: UpstreamProtocol,
    expected: AuthenticationScheme,
) {
    let (selected, other, value) = match expected {
        asb_core::AuthenticationScheme::XGoogApiKey => {
            panic!("Google native has a separate Claude fixture")
        }
        AuthenticationScheme::Bearer => {
            ("authorization", "x-api-key", format!("Bearer {TEST_KEY}"))
        }
        AuthenticationScheme::XApiKey => ("x-api-key", "authorization", TEST_KEY.to_string()),
    };
    assert_eq!(headers.get_all(selected).iter().count(), 1);
    assert!(
        headers[selected].to_str().unwrap() == value,
        "selected credential mismatch"
    );
    assert!(
        !headers.contains_key(other),
        "unselected authentication header was emitted"
    );
    if protocol == UpstreamProtocol::AnthropicMessages {
        assert_eq!(headers.get_all("anthropic-version").iter().count(), 1);
        assert_eq!(headers["anthropic-version"], "2023-06-01");
    } else {
        assert!(!headers.contains_key("anthropic-version"));
    }
    assert_eq!(headers[CONTENT_TYPE], "application/json");
    assert_eq!(headers[ACCEPT], "application/json");
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
    let bearer = header_values(request, "authorization");
    let api_key = header_values(request, "x-api-key");
    let auth = match expected {
        asb_core::AuthenticationScheme::XGoogApiKey => {
            panic!("Google native has a separate Claude fixture")
        }
        AuthenticationScheme::Bearer => {
            bearer == [format!("Bearer {TEST_KEY}")] && api_key.is_empty()
        }
        AuthenticationScheme::XApiKey => api_key == [TEST_KEY] && bearer.is_empty(),
    };
    let version = header_values(request, "anthropic-version");
    auth && if protocol == UpstreamProtocol::AnthropicMessages {
        version == ["2023-06-01"]
    } else {
        version.is_empty()
    }
}

fn fixture(
    protocol: UpstreamProtocol,
    expected: AuthenticationScheme,
    status: u16,
) -> (String, JoinHandle<bool>) {
    let server = Server::http(("127.0.0.1", 0)).expect("bind loopback fixture");
    let endpoint = format!(
        "http://{}{}",
        server.server_addr().to_ip().unwrap(),
        endpoint_path(protocol)
    );
    let worker = thread::spawn(move || {
        let request = server
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .expect("bounded provider request");
        let accepted = request.method() == &Method::Post
            && request.url() == endpoint_path(protocol)
            && accepts_headers(&request, protocol, expected);
        let body = if accepted && status == 200 {
            success_body(protocol, "verified")
        } else {
            json!({ "error": { "message": format!("rejected fixture credential {TEST_KEY}") } })
        };
        request
            .respond(
                Response::from_string(body.to_string())
                    .with_status_code(if accepted { status } else { 401 })
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
                    .with_header(
                        Header::from_bytes("x-request-id", format!("req-{TEST_KEY}")).unwrap(),
                    ),
            )
            .expect("reply to bounded provider request");
        accepted
    });
    (endpoint, worker)
}

#[test]
fn builder_uses_selected_authentication_and_independent_anthropic_version() {
    let client = direct_client();
    for protocol in PROTOCOLS {
        for selected in SELECTIONS {
            let connection = connection(protocol, selected);
            let endpoint = format!("http://127.0.0.1:1{}", endpoint_path(protocol));
            let request =
                build_request(&client, &endpoint, &connection, protocol, "requested-model")
                    .unwrap();
            assert_built_headers(
                request.headers(),
                protocol,
                expected_auth(protocol, selected),
            );
            assert_eq!(request.method(), &reqwest::Method::POST);
            assert_eq!(request.url().path(), endpoint_path(protocol));
            assert!(!request.url().as_str().contains(TEST_KEY));
            let bytes = request.body().unwrap().as_bytes().unwrap();
            let body: Value = serde_json::from_slice(bytes).unwrap();
            assert_eq!(body["model"], "requested-model");
            assert_eq!(body["stream"], false);
            assert!(!String::from_utf8_lossy(bytes).contains(TEST_KEY));
        }
    }
}

#[test]
fn send_succeeds_only_with_the_selected_header_for_each_protocol() {
    for protocol in PROTOCOLS {
        for selected in SELECTIONS {
            let (endpoint, worker) = fixture(protocol, expected_auth(protocol, selected), 200);
            let result = tauri::async_runtime::block_on(send(
                direct_client(),
                connection(protocol, selected),
                endpoint,
                "requested-model".to_string(),
                Instant::now(),
            ));
            assert!(
                worker.join().unwrap(),
                "wrong auth or protocol-version header"
            );
            let result = result.unwrap();
            assert_eq!(result.outcome, Outcome::Success);
            assert_eq!(result.reply.as_deref(), Some("verified"));
            assert_eq!(result.status, Some(200));
            assert!(!serde_json::to_string(&result).unwrap().contains(TEST_KEY));
        }
    }
}

#[test]
fn strict_request_fixture_rejects_wrong_and_dual_authentication() {
    let client = direct_client();
    for protocol in PROTOCOLS {
        for expected in [AuthenticationScheme::Bearer, AuthenticationScheme::XApiKey] {
            for dual in [false, true] {
                let (endpoint, worker) = fixture(protocol, expected, 200);
                let connection = connection(protocol, Some(expected));
                let mut request =
                    build_request(&client, &endpoint, &connection, protocol, "model").unwrap();
                let (selected, other) = match expected {
                    asb_core::AuthenticationScheme::XGoogApiKey => {
                        panic!("Google native has a separate Claude fixture")
                    }
                    AuthenticationScheme::Bearer => ("authorization", "x-api-key"),
                    AuthenticationScheme::XApiKey => ("x-api-key", "authorization"),
                };
                if !dual {
                    request.headers_mut().remove(selected);
                }
                request
                    .headers_mut()
                    .insert(other, "unselected-fixture-credential".parse().unwrap());
                let result =
                    tauri::async_runtime::block_on(async { client.execute(request).await });
                assert!(
                    !worker.join().unwrap(),
                    "fixture accepted unselected authentication"
                );
                assert_eq!(result.unwrap().status().as_u16(), 401);
            }
        }
    }
}

#[test]
fn selected_auth_errors_remain_classified_and_redact_upstream_echoes() {
    for protocol in PROTOCOLS {
        for selected in SELECTIONS {
            let (endpoint, worker) = fixture(protocol, expected_auth(protocol, selected), 401);
            let result = tauri::async_runtime::block_on(send(
                direct_client(),
                connection(protocol, selected),
                endpoint,
                "model".to_string(),
                Instant::now(),
            ));
            assert!(worker.join().unwrap(), "fixture rejected request headers");
            let result = result.unwrap();
            assert_eq!(result.outcome, Outcome::AuthenticationFailed);
            assert_eq!(result.status, Some(401));
            assert!(result.reply.is_none());
            assert!(!serde_json::to_string(&result).unwrap().contains(TEST_KEY));
        }
    }
}

#[test]
fn builder_and_send_reject_invalid_credentials_before_network_without_leaks() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = direct_client();
    for protocol in PROTOCOLS {
        for selected in SELECTIONS {
            for suffix in [
                "\r\nx-injected: fixture",
                "\nfixture",
                "\0fixture",
                "\u{7f}fixture",
            ] {
                let mut connection = connection(protocol, selected);
                connection.api_key = format!("{TEST_KEY}{suffix}");
                let endpoint = format!(
                    "http://{}{}",
                    listener.local_addr().unwrap(),
                    endpoint_path(protocol)
                );
                let error = build_request(&client, &endpoint, &connection, protocol, "model")
                    .err()
                    .expect("invalid request header must fail");
                assert_invalid_error(&error);
                let error = tauri::async_runtime::block_on(send(
                    client.clone(),
                    connection,
                    endpoint,
                    "model".to_string(),
                    Instant::now(),
                ))
                .unwrap_err();
                assert_invalid_error(&error);
                assert_eq!(
                    listener.accept().unwrap_err().kind(),
                    std::io::ErrorKind::WouldBlock
                );
            }
        }
    }
}

fn assert_invalid_error(error: &CommandError) {
    assert_eq!(error.code, "provider-request-invalid");
    let serialized = serde_json::to_string(error).unwrap();
    assert!(
        !serialized.contains(TEST_KEY),
        "invalid-header diagnostic exposed key text"
    );
    assert!(!serialized.contains("x-injected"));
}
