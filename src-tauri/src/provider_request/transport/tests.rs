use super::*;
use crate::provider_request::tests::{direct_client, record, success_body, TEST_KEY};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn read_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0; 4_096];
    loop {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "request ended before its payload");
        bytes.extend_from_slice(&buffer[..read]);
        let Some(boundary) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&bytes[..boundary]);
        let length: usize = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().unwrap())
            })
            .unwrap_or(0);
        if bytes.len() >= boundary + 4 + length {
            break;
        }
    }
    String::from_utf8(bytes).unwrap()
}

fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let started = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    started.elapsed() < Duration::from_secs(3),
                    "request was not sent"
                );
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("accept failed: {error}"),
        }
    }
}

pub(super) fn fixture(raw_response: Vec<u8>) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1/test", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        let request = read_request(&mut stream);
        let _ = stream.write_all(&raw_response);
        request
    });
    (url, server)
}

pub(super) fn http(status: u16, body: &str) -> Vec<u8> {
    format!("HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes()
}

pub(super) fn execute_at(protocol: UpstreamProtocol, url: String) -> ProviderRequestResult {
    tauri::async_runtime::block_on(send(
        direct_client(),
        crate::provider_request::ProviderRequestConnection::try_from(&record(protocol).profile)
            .unwrap(),
        url,
        "requested-model".to_string(),
        Instant::now(),
    ))
    .unwrap()
}

#[test]
fn protocol_requests_use_their_only_authentication_and_fixed_payload() {
    for protocol in [
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::AnthropicMessages,
    ] {
        let body = success_body(protocol, "连接成功。").to_string();
        let (url, server) = fixture(http(200, &body));
        let result = execute_at(protocol, url);
        assert_eq!(result.outcome, Outcome::Success);
        assert_eq!(result.model.as_deref(), Some("actual-model"));
        assert_eq!(result.reply.as_deref(), Some("连接成功。"));
        let raw = server.join().unwrap();
        let (headers, body) = raw.split_once("\r\n\r\n").unwrap();
        let headers = headers.to_ascii_lowercase();
        assert!(headers.starts_with("post "));
        assert!(headers.contains("content-type: application/json"));
        if protocol == UpstreamProtocol::AnthropicMessages {
            assert!(headers.contains(&format!("x-api-key: {TEST_KEY}")));
            assert!(headers.contains("anthropic-version: 2023-06-01"));
            assert!(!headers.contains("authorization:"));
        } else {
            assert!(headers.contains(&format!("authorization: bearer {TEST_KEY}")));
            assert!(!headers.contains("x-api-key:"));
        }
        let body: Value = serde_json::from_str(body).unwrap();
        assert_eq!(body["model"], "requested-model");
        assert_eq!(body["stream"], false);
        assert!(body.to_string().contains(REQUEST_PROMPT));
        let cap = match protocol {
            UpstreamProtocol::Responses => "max_output_tokens",
            UpstreamProtocol::ChatCompletions => "max_completion_tokens",
            UpstreamProtocol::AnthropicMessages => "max_tokens",
        };
        assert_eq!(body[cap], MAX_OUTPUT_TOKENS);
    }
}

#[test]
fn http_failures_are_classified_and_never_echo_credentials() {
    for (status, outcome) in [
        (401, Outcome::AuthenticationFailed),
        (403, Outcome::AuthenticationFailed),
        (429, Outcome::RateLimited),
        (500, Outcome::HttpError),
    ] {
        let (url, server) = fixture(http(
            status,
            &json!({"error":{"message":format!("key '{TEST_KEY}' rejected")}}).to_string(),
        ));
        let result = execute_at(UpstreamProtocol::Responses, url);
        assert_eq!(result.outcome, outcome);
        assert_eq!(result.status, Some(status));
        assert!(result.reply.is_none());
        assert!(!serde_json::to_string(&result).unwrap().contains(TEST_KEY));
        server.join().unwrap();
    }
}

#[test]
fn html_with_http_200_and_error_json_are_not_model_successes() {
    for body in [
        "<html>Service online</html>",
        "{\"error\":{\"message\":\"model unavailable\"}}",
    ] {
        let (url, server) = fixture(http(200, body));
        let result = execute_at(UpstreamProtocol::Responses, url);
        assert_eq!(result.status, Some(200));
        assert_eq!(result.outcome, Outcome::InvalidResponse);
        server.join().unwrap();
    }
}

#[test]
fn response_limits_apply_to_content_length_and_chunked_bodies() {
    let body = "x".repeat(MAX_RESPONSE_BYTES + 1);
    let chunked = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
        body.len()
    );
    for raw in [http(200, &body), chunked.into_bytes()] {
        let (url, server) = fixture(raw);
        let result = execute_at(UpstreamProtocol::Responses, url);
        assert_eq!(result.outcome, Outcome::InvalidResponse);
        assert!(result.error.unwrap().contains("64 KiB"));
        server.join().unwrap();
    }
}

#[test]
fn redirects_do_not_forward_the_credential_or_send_another_request() {
    let destination = TcpListener::bind("127.0.0.1:0").unwrap();
    destination.set_nonblocking(true).unwrap();
    let redirect = format!(
        "HTTP/1.1 302 Found\r\nLocation: http://{}/other\r\nContent-Length: 0\r\n\r\n",
        destination.local_addr().unwrap()
    );
    let (url, server) = fixture(redirect.into_bytes());
    let result = execute_at(UpstreamProtocol::Responses, url);
    assert_eq!(result.outcome, Outcome::HttpError);
    assert_eq!(result.status, Some(302));
    server.join().unwrap();
    assert_eq!(
        destination.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn timeout_while_reading_a_response_stops_the_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        read_request(&mut stream);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n{")
            .unwrap();
        assert_socket_closed(&mut stream);
    });
    let client = client_builder()
        .no_proxy()
        .timeout(Duration::from_millis(200))
        .build()
        .unwrap();
    let result = tauri::async_runtime::block_on(send(
        client,
        crate::provider_request::ProviderRequestConnection::try_from(
            &record(UpstreamProtocol::Responses).profile,
        )
        .unwrap(),
        url,
        "model".to_string(),
        Instant::now(),
    ))
    .unwrap();
    assert_eq!(result.outcome, Outcome::Timeout);
    assert_eq!(result.status, Some(200));
    server.join().unwrap();
}

fn assert_socket_closed(stream: &mut TcpStream) {
    let mut byte = [0];
    match stream.read(&mut byte) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ) => {}
        result => panic!("HTTP socket was not closed: {result:?}"),
    }
}

#[test]
fn cancellation_while_reading_a_response_closes_the_socket_before_returning() {
    use crate::config_store::ConfigStore;
    use crate::provider_request::{
        execute_with_client, prepare, ProviderRequestInput, ProviderRequestTarget, ProviderRequests,
    };
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        read_request(&mut stream);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n{")
            .unwrap();
        ready_tx.send(()).unwrap();
        assert_socket_closed(&mut stream);
    });
    let record = store
        .create_provider(crate::provider_request::tests::draft(
            UpstreamProtocol::Responses,
            &url,
        ))
        .unwrap();
    let requests = ProviderRequests::default();
    let preparation = prepare(
        &requests,
        &store,
        ProviderRequestTarget::Saved {
            profile_id: record.profile.id.clone(),
        },
    )
    .unwrap();
    let execution = tauri::async_runtime::spawn(execute_with_client(
        requests.clone(),
        store,
        ProviderRequestInput {
            request_id: preparation.request_id.clone(),
            model: "model".to_string(),
        },
        direct_client(),
    ));
    ready_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(tauri::async_runtime::block_on(requests.cancel(&preparation.request_id)).unwrap());
    let result = tauri::async_runtime::block_on(execution).unwrap().unwrap();
    assert_eq!(result.outcome, Outcome::Cancelled);
    server.join().unwrap();
}
