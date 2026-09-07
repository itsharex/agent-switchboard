use super::respond::content_type;
use super::*;
use crate::gateway::GatewayController;
use crate::local_state::LocalState;
use asb_core::contracts::{
    AppKind, AuthenticationScheme, CommonSettingValue, ConfigValue, ProviderDraft, RouteMode,
    SwitchPlan, UpstreamProtocol,
};
use asb_core::ownership::default_common_settings;
use asb_switch::io::FsIo;
use reqwest::header::CONTENT_TYPE;
use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Instant;
use tiny_http::Response;

fn endpoint(server: &Server) -> String {
    let port = server
        .server_addr()
        .to_ip()
        .expect("loopback address")
        .port();
    format!("http://127.0.0.1:{port}")
}

fn websocket_connect(address: &str, token: &str) -> TcpStream {
    let mut stream = TcpStream::connect(address).expect("connect WebSocket client");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set WebSocket read timeout");
    stream
            .write_all(
                format!(
                    "GET /v1/responses HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nAuthorization: Bearer {token}\r\n\r\n"
                )
                .as_bytes(),
            )
            .expect("write WebSocket handshake");
    let mut response = Vec::new();
    let mut byte = [0_u8; 1];
    while !response.windows(4).any(|window| window == b"\r\n\r\n") {
        let count = stream.read(&mut byte).expect("read WebSocket handshake");
        assert_eq!(count, 1, "WebSocket handshake ended early");
        response.push(byte[0]);
    }
    let response = String::from_utf8(response).expect("WebSocket handshake UTF-8");
    assert!(response.starts_with("HTTP/1.1 101"));
    assert!(response.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
    stream
}

fn send_websocket_text(stream: &mut TcpStream, text: &str) {
    let bytes = text.as_bytes();
    assert!(bytes.len() < u16::MAX as usize, "test frame is bounded");
    let mut frame = vec![0x81];
    match bytes.len() {
        0..=125 => frame.push(0x80 | bytes.len() as u8),
        length => {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(length as u16).to_be_bytes());
        }
    }
    let mask = [0x12_u8, 0x34, 0x56, 0x78];
    frame.extend_from_slice(&mask);
    frame.extend(
        bytes
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4]),
    );
    stream.write_all(&frame).expect("write WebSocket frame");
    stream.flush().expect("flush WebSocket frame");
}

fn read_websocket_text(stream: &mut TcpStream) -> String {
    let mut first = [0_u8; 2];
    stream
        .read_exact(&mut first)
        .expect("read WebSocket frame header");
    assert_eq!(first[0] & 0x0f, 0x1, "gateway must emit text frames");
    assert_eq!(first[1] & 0x80, 0, "gateway frames must be unmasked");
    let length = match first[1] & 0x7f {
        length @ 0..=125 => length as usize,
        126 => {
            let mut bytes = [0_u8; 2];
            stream
                .read_exact(&mut bytes)
                .expect("read extended WebSocket length");
            u16::from_be_bytes(bytes) as usize
        }
        127 => {
            let mut bytes = [0_u8; 8];
            stream
                .read_exact(&mut bytes)
                .expect("read long WebSocket length");
            usize::try_from(u64::from_be_bytes(bytes)).expect("bounded WebSocket frame")
        }
        _ => unreachable!(),
    };
    let mut bytes = vec![0_u8; length];
    stream
        .read_exact(&mut bytes)
        .expect("read WebSocket frame body");
    String::from_utf8(bytes).expect("WebSocket text UTF-8")
}

fn websocket_request(input: Value, previous_response_id: Option<&str>) -> String {
    let mut request = json!({
        "type": "response.create",
        "model": "sandbox-model",
        "instructions": "system instructions",
        "input": input,
        "tools": [{
            "type": "function",
            "name": "sandbox_tool",
            "parameters": { "type": "object", "properties": {} },
            "strict": false,
        }],
        "tool_choice": "auto",
        "parallel_tool_calls": true,
        "reasoning": { "summary": "auto" },
        "store": false,
        "stream": true,
        "include": ["reasoning.encrypted_content"],
        "prompt_cache_key": "sandbox-thread",
        "client_metadata": { "session_id": "sandbox-session" },
        "generate": true,
    });
    if let Some(previous_response_id) = previous_response_id {
        request["previous_response_id"] = Value::String(previous_response_id.to_string());
    }
    request.to_string()
}

fn receive_responses_completion(stream: &mut TcpStream) -> Value {
    for _ in 0..32 {
        let event: Value = serde_json::from_str(&read_websocket_text(stream))
            .expect("WebSocket Responses event JSON");
        if event["type"] == "response.completed" {
            return event;
        }
        assert_ne!(
            event["type"], "response.failed",
            "gateway must not fail the route"
        );
    }
    panic!("WebSocket did not complete within the event limit");
}

fn client_request(app: AppKind) -> Value {
    match app {
        AppKind::Codex => json!({
            "model": "sandbox-model",
            "input": "hello through the gateway",
            "max_output_tokens": 32,
        }),
        AppKind::Claude => json!({
            "model": "sandbox-model",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello through the gateway" }],
        }),
    }
}

fn upstream_response(protocol: UpstreamProtocol) -> Value {
    match protocol {
        UpstreamProtocol::Responses => json!({
            "id": "resp_sandbox",
            "object": "response",
            "status": "completed",
            "model": "sandbox-model",
            "output": [{
                "type": "message",
                "id": "msg_sandbox",
                "status": "completed",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "converted upstream answer", "annotations": [] }],
            }],
            "usage": { "input_tokens": 3, "output_tokens": 4, "total_tokens": 7 },
            "error": null,
        }),
        UpstreamProtocol::ChatCompletions => json!({
            "id": "chat_sandbox",
            "object": "chat.completion",
            "created": 0,
            "model": "sandbox-model",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "converted upstream answer" },
                "finish_reason": "stop",
            }],
            "usage": { "prompt_tokens": 3, "completion_tokens": 4, "total_tokens": 7 },
        }),
        UpstreamProtocol::AnthropicMessages => json!({
            "id": "msg_sandbox",
            "type": "message",
            "role": "assistant",
            "model": "sandbox-model",
            "content": [{"type": "text", "text": "converted upstream answer"}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 3, "output_tokens": 4},
        }),
    }
}

fn assert_upstream_request(protocol: UpstreamProtocol, body: &Value) {
    match protocol {
        UpstreamProtocol::Responses => {
            assert_eq!(body["input"][0]["role"], "user");
            assert_eq!(
                body["input"][0]["content"][0]["text"],
                "hello through the gateway"
            );
        }
        UpstreamProtocol::ChatCompletions => {
            assert_eq!(body["messages"][0]["role"], "user");
            assert_eq!(body["messages"][0]["content"], "hello through the gateway");
        }
        UpstreamProtocol::AnthropicMessages => {
            assert_eq!(body["messages"][0]["role"], "user");
            assert_eq!(
                body["messages"][0]["content"][0]["text"],
                "hello through the gateway"
            );
        }
    }
}

fn run_cross_protocol_case(app: AppKind, upstream_protocol: UpstreamProtocol) {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let upstream_url = endpoint(&upstream);
    let upstream_key = format!("fixture-{}", uuid::Uuid::new_v4());
    let (observed_sender, observed_receiver) = mpsc::channel();
    let upstream_worker = thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(10))
            .expect("upstream receive")
            .expect("upstream request");
        let path = request.url().to_string();
        let authorization = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("Authorization"))
            .map(|header| header.value.as_str().to_string());
        let x_api_key = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("x-api-key"))
            .map(|header| header.value.as_str().to_string());
        let anthropic_version = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("anthropic-version"))
            .map(|header| header.value.as_str().to_string());
        let mut body = String::new();
        request
            .as_reader()
            .read_to_string(&mut body)
            .expect("read upstream body");
        observed_sender
            .send((path, authorization, x_api_key, anthropic_version, body))
            .expect("report upstream request");
        let response = Response::from_data(
            serde_json::to_vec(&upstream_response(upstream_protocol)).expect("response json"),
        )
        .with_header(content_type("application/json"));
        request.respond(response).expect("respond upstream");
    });

    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let record = state
        .configuration()
        .create_provider(ProviderDraft {
            app,
            route_mode: RouteMode::Custom,
            name: "cross protocol sandbox".to_string(),
            base_url: Some(upstream_url),
            api_key: upstream_key.clone(),
            upstream_protocol: Some(upstream_protocol),
            max_output_tokens: ((app == AppKind::Codex
                && upstream_protocol == UpstreamProtocol::AnthropicMessages)
                .then_some(8_192))
            .into(),
            model: Some("sandbox-model".to_string()),
            model_options: None,
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        })
        .expect("create provider");
    let gateway = GatewayController::start(&state).expect("start gateway");
    let projection = gateway
        .project(&SwitchPlan::direct(
            record.profile,
            default_common_settings(app),
        ))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("commit route");
    let client = Client::builder().no_proxy().build().expect("client");
    let gateway_url = match app {
        AppKind::Codex => format!("{}/v1/responses", gateway.inner.base_url),
        AppKind::Claude => format!("{}/v1/messages", gateway.inner.base_url),
    };
    let response = client
        .post(gateway_url)
        .header(
            "Authorization",
            format!("Bearer {}", projection.plan.profile.api_key),
        )
        .header(CONTENT_TYPE, "application/json")
        .body(serde_json::to_vec(&client_request(app)).expect("client request json"))
        .send()
        .expect("gateway request");
    assert!(response.status().is_success());
    let client_body: Value = serde_json::from_str(&response.text().expect("client response"))
        .expect("client response json");
    match app {
        AppKind::Codex => {
            assert_eq!(client_body["object"], "response");
            assert_eq!(
                client_body["output"][0]["content"][0]["text"],
                "converted upstream answer"
            );
        }
        AppKind::Claude => {
            assert_eq!(client_body["type"], "message");
            assert_eq!(
                client_body["content"][0]["text"],
                "converted upstream answer"
            );
        }
    }

    let (path, authorization, x_api_key, anthropic_version, body) = observed_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("one upstream request");
    let expected_path = match upstream_protocol {
        UpstreamProtocol::Responses => "/v1/responses",
        UpstreamProtocol::ChatCompletions => "/v1/chat/completions",
        UpstreamProtocol::AnthropicMessages => "/v1/messages",
    };
    assert_eq!(path, expected_path);
    match upstream_protocol.authentication_scheme() {
        AuthenticationScheme::Bearer => {
            assert_eq!(authorization, Some(format!("Bearer {upstream_key}")));
            assert!(x_api_key.is_none());
            assert!(anthropic_version.is_none());
        }
        AuthenticationScheme::XApiKey => {
            assert!(authorization.is_none());
            assert_eq!(x_api_key.as_deref(), Some(upstream_key.as_str()));
            assert_eq!(anthropic_version.as_deref(), Some("2023-06-01"));
        }
    }
    assert_upstream_request(
        upstream_protocol,
        &serde_json::from_str(&body).expect("upstream request json"),
    );
    upstream_worker.join().expect("upstream worker");
    gateway.shutdown();
}

#[test]
fn loopback_gateway_converts_every_cross_protocol_pair() {
    for (app, upstream_protocol) in [
        (AppKind::Codex, UpstreamProtocol::ChatCompletions),
        (AppKind::Codex, UpstreamProtocol::AnthropicMessages),
        (AppKind::Claude, UpstreamProtocol::Responses),
        (AppKind::Claude, UpstreamProtocol::ChatCompletions),
    ] {
        run_cross_protocol_case(app, upstream_protocol);
    }
}

#[test]
fn loopback_gateway_streams_a_converted_first_event_before_upstream_finishes() {
    let upstream = TcpListener::bind(("127.0.0.1", 0)).expect("upstream listener");
    let upstream_url = format!(
        "http://{}",
        upstream.local_addr().expect("upstream address")
    );
    let upstream_key = format!("fixture-{}", uuid::Uuid::new_v4());
    let (release_sender, release_receiver) = mpsc::channel();
    let (upstream_first_sender, upstream_first_receiver) = mpsc::channel();
    let upstream_worker = thread::spawn(move || {
        let (mut stream, _) = upstream.accept().expect("upstream receive");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set read timeout");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = stream.read(&mut buffer).expect("read upstream request");
            assert!(count > 0, "request ended before headers");
            request.extend_from_slice(&buffer[..count]);
        }
        let headers_end = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("request headers end");
        let content_length = std::str::from_utf8(&request[..headers_end])
            .expect("request headers UTF-8")
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("Content-Length")
                        .then(|| value.trim().parse::<usize>().expect("content length"))
                })
            })
            .expect("request content length");
        let request_end = headers_end + 4 + content_length;
        while request.len() < request_end {
            let count = stream
                .read(&mut buffer)
                .expect("read upstream request body");
            assert!(count > 0, "request ended before body");
            request.extend_from_slice(&buffer[..count]);
        }
        let first = br#"data: {"id":"chat_stream","object":"chat.completion.chunk","created":0,"model":"sandbox-model","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

"#
            .to_vec();
        let rest = br#"data: {"id":"chat_stream","object":"chat.completion.chunk","created":0,"model":"sandbox-model","choices":[{"index":0,"delta":{"content":"streamed answer"},"finish_reason":null}]}

data: {"id":"chat_stream","object":"chat.completion.chunk","created":0,"model":"sandbox-model","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

"#
            .to_vec();
        stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                )
                .expect("write headers");
        write!(stream, "{:x}\r\n", first.len()).expect("write first chunk length");
        stream.write_all(&first).expect("write first chunk");
        stream.write_all(b"\r\n").expect("finish first chunk");
        stream.flush().expect("flush first chunk");
        upstream_first_sender
            .send(())
            .expect("report upstream first chunk");
        release_receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("test stream release");
        write!(stream, "{:x}\r\n", rest.len()).expect("write remaining chunk length");
        stream.write_all(&rest).expect("write remaining chunk");
        stream.write_all(b"\r\n0\r\n\r\n").expect("finish response");
        stream.flush().expect("flush response");
    });

    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let record = state
        .configuration()
        .create_provider(ProviderDraft {
            app: AppKind::Codex,
            route_mode: RouteMode::Custom,
            name: "streaming chat sandbox".to_string(),
            base_url: Some(upstream_url),
            api_key: upstream_key,
            upstream_protocol: Some(UpstreamProtocol::ChatCompletions),
            max_output_tokens: None.into(),
            model: Some("sandbox-model".to_string()),
            model_options: None,
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        })
        .expect("create provider");
    let gateway = GatewayController::start(&state).expect("start gateway");
    let projection = gateway
        .project(&SwitchPlan::direct(
            record.profile,
            default_common_settings(AppKind::Codex),
        ))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("commit route");

    let gateway_url = format!("{}/v1/responses", gateway.inner.base_url);
    let local_token = projection.plan.profile.api_key;
    let (first_sender, first_receiver) = mpsc::channel();
    let client_worker = thread::spawn(move || {
        let client = Client::builder().no_proxy().build().expect("client");
        let mut response = client
            .post(gateway_url)
            .header("Authorization", format!("Bearer {local_token}"))
            .header(CONTENT_TYPE, "application/json")
            .body(
                serde_json::to_vec(&json!({
                    "model": "sandbox-model",
                    "input": "stream through gateway",
                    "max_output_tokens": 32,
                    "stream": true,
                }))
                .expect("request json"),
            )
            .send()
            .expect("gateway request");
        assert!(response.status().is_success());
        let mut first = [0_u8; 4096];
        let count = response.read(&mut first).expect("first stream bytes");
        first_sender
            .send(String::from_utf8(first[..count].to_vec()).expect("first utf8"))
            .expect("report first frame");
        let mut rest = String::new();
        response
            .read_to_string(&mut rest)
            .expect("remaining stream");
        rest
    });

    upstream_first_receiver
        .recv_timeout(Duration::from_secs(3))
        .expect("upstream must flush its first chunk");
    let first = first_receiver
        .recv_timeout(Duration::from_secs(3))
        .expect("gateway must emit before upstream release");
    assert!(first.contains("response.created"));
    assert!(!first.contains("streamed answer"));
    release_sender.send(()).expect("release upstream stream");
    let rest = client_worker.join().expect("client worker");
    assert!(rest.contains("response.output_text.delta"), "{rest}");
    assert!(rest.contains("streamed answer"), "{rest}");
    assert!(rest.contains("response.completed"), "{rest}");
    upstream_worker.join().expect("upstream worker");
    gateway.shutdown();
}

#[test]
fn codex_websocket_replays_visible_context_and_keeps_upstream_credentials_private() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let upstream_url = endpoint(&upstream);
    let upstream_key = format!("fixture-{}", uuid::Uuid::new_v4());
    let (observed_sender, observed_receiver) = mpsc::channel();
    let upstream_worker = thread::spawn(move || {
        for number in 1..=2 {
            let mut request = upstream
                .recv_timeout(Duration::from_secs(10))
                .expect("upstream receive")
                .expect("upstream request");
            let authorization = request
                .headers()
                .iter()
                .find(|header| header.field.equiv("Authorization"))
                .map(|header| header.value.as_str().to_string());
            let mut body = String::new();
            request
                .as_reader()
                .read_to_string(&mut body)
                .expect("read upstream request");
            observed_sender
                .send((authorization, body))
                .expect("report upstream request");
            let sse = format!(
                    "data: {{\"id\":\"chat_ws_{number}\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{{\"index\":0,\"delta\":{{\"role\":\"assistant\"}},\"finish_reason\":null}}]}}\n\ndata: {{\"id\":\"chat_ws_{number}\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{{\"index\":0,\"delta\":{{\"reasoning_content\":\"private reasoning {number}\"}},\"finish_reason\":null}}]}}\n\ndata: {{\"id\":\"chat_ws_{number}\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"answer {number}\"}},\"finish_reason\":null}}]}}\n\ndata: {{\"id\":\"chat_ws_{number}\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: [DONE]\n\n"
                );
            request
                .respond(
                    Response::from_data(sse.into_bytes())
                        .with_header(content_type("text/event-stream; charset=utf-8")),
                )
                .expect("respond upstream");
        }
    });

    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let record = state
        .configuration()
        .create_provider(ProviderDraft {
            app: AppKind::Codex,
            route_mode: RouteMode::Custom,
            name: "WebSocket chat sandbox".to_string(),
            base_url: Some(upstream_url),
            api_key: upstream_key.clone(),
            upstream_protocol: Some(UpstreamProtocol::ChatCompletions),
            max_output_tokens: None.into(),
            model: Some("sandbox-model".to_string()),
            model_options: None,
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        })
        .expect("create provider");
    let gateway = GatewayController::start(&state).expect("start gateway");
    let mut common = default_common_settings(AppKind::Codex);
    common.settings.insert(
        "web_search".to_string(),
        CommonSettingValue::Explicit {
            value: ConfigValue::Str("live".to_string()),
        },
    );
    let projection = gateway
        .project(&SwitchPlan::direct(record.profile, common))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("activate routed gateway");
    let address = gateway
        .inner
        .base_url
        .strip_prefix("http://")
        .expect("loopback HTTP address")
        .to_string();
    let local_token = projection.plan.profile.api_key;
    let mut socket = websocket_connect(&address, &local_token);

    send_websocket_text(
        &mut socket,
        &websocket_request(
            json!([{
                "type": "message",
                "id": "msg-first",
                "role": "user",
                "content": [{ "type": "input_text", "text": "first question" }],
                "internal_chat_message_metadata_passthrough": { "turn_id": "turn-first" },
            }]),
            None,
        ),
    );
    let first = receive_responses_completion(&mut socket);
    assert_eq!(first["response"]["id"], "chat_ws_1");
    assert!(!first.to_string().contains(&upstream_key));
    assert!(!first.to_string().contains("private reasoning 1"));

    send_websocket_text(
        &mut socket,
        &websocket_request(
            json!([{
                "type": "message",
                "id": "msg-second",
                "role": "user",
                "content": [{ "type": "input_text", "text": "second question" }],
                "internal_chat_message_metadata_passthrough": { "turn_id": "turn-second" },
            }]),
            Some("chat_ws_1"),
        ),
    );
    let second = receive_responses_completion(&mut socket);
    assert_eq!(second["response"]["id"], "chat_ws_2");
    assert!(!second.to_string().contains(&upstream_key));
    assert!(!second.to_string().contains("private reasoning 2"));

    let (first_authorization, first_body) = observed_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("first upstream request");
    let (second_authorization, second_body) = observed_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("second upstream request");
    assert_eq!(first_authorization, Some(format!("Bearer {upstream_key}")));
    assert_eq!(second_authorization, Some(format!("Bearer {upstream_key}")));
    for body in [&first_body, &second_body] {
        assert!(!body.contains(&local_token));
        assert!(!body.contains("client_metadata"));
        assert!(!body.contains("prompt_cache_key"));
        assert!(!body.contains("previous_response_id"));
    }
    assert!(!first_body.contains("reasoning_content"));
    let first_body: Value = serde_json::from_str(&first_body).expect("first upstream JSON");
    assert_eq!(first_body["parallel_tool_calls"], true);
    assert_eq!(first_body["messages"][0]["role"], "system");
    assert_eq!(first_body["messages"][1]["content"], "first question");
    let second_body: Value = serde_json::from_str(&second_body).expect("second upstream JSON");
    let messages = second_body["messages"].as_array().expect("Chat messages");
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[1]["content"], "first question");
    assert_eq!(messages[2]["role"], "assistant");
    assert_eq!(messages[2]["reasoning_content"], "private reasoning 1");
    assert_eq!(messages[2]["content"], "answer 1");
    assert_eq!(messages[3]["content"], "second question");
    upstream_worker.join().expect("upstream worker");
    gateway.shutdown();
}

#[test]
#[ignore = "requires an installed Codex CLI and an isolated loopback sandbox"]
fn actual_codex_cli_completes_through_the_isolated_websocket_gateway() {
    Command::new("codex")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("the explicit black-box test requires the Codex CLI");

    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let upstream_url = endpoint(&upstream);
    let upstream_key = format!("fixture-{}", uuid::Uuid::new_v4());
    let (observed_sender, observed_receiver) = mpsc::channel();
    let upstream_worker = thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(30))
            .expect("upstream receive")
            .expect("upstream request");
        let authorization = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("Authorization"))
            .map(|header| header.value.as_str().to_string());
        let mut body = String::new();
        request
            .as_reader()
            .read_to_string(&mut body)
            .expect("read upstream request");
        observed_sender
            .send((authorization, body))
            .expect("report upstream request");
        let stream = b"data: {\"id\":\"chat_cli\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_cli\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"private sandbox reasoning\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_cli\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"sandbox answer\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_cli\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        request
            .respond(
                Response::from_data(stream.as_slice())
                    .with_header(content_type("text/event-stream; charset=utf-8")),
            )
            .expect("respond upstream");
    });

    let directory = tempfile::tempdir().expect("temporary sandbox root");
    let state = LocalState::from_root(directory.path().join("state"));
    let record = state
        .configuration()
        .create_provider(ProviderDraft {
            app: AppKind::Codex,
            route_mode: RouteMode::Custom,
            name: "Codex CLI black-box sandbox".to_string(),
            base_url: Some(upstream_url),
            api_key: upstream_key.clone(),
            upstream_protocol: Some(UpstreamProtocol::ChatCompletions),
            max_output_tokens: None.into(),
            model: Some("sandbox-model".to_string()),
            model_options: None,
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        })
        .expect("create provider");
    let gateway = GatewayController::start(&state).expect("start gateway");
    let mut common = default_common_settings(AppKind::Codex);
    common.settings.insert(
        "web_search".to_string(),
        CommonSettingValue::Explicit {
            value: ConfigValue::Str("live".to_string()),
        },
    );
    let projection = gateway
        .project(&SwitchPlan::direct(record.profile, common))
        .expect("project route");

    let codex_home = directory.path().join("codex-home");
    let workdir = directory.path().join("work");
    fs::create_dir_all(&codex_home).expect("create temporary Codex home");
    fs::create_dir_all(&workdir).expect("create temporary working directory");
    let config = codex_home.join("config.toml");
    let auth = codex_home.join("auth.json");
    let backup_dir = directory.path().join("backups");
    let io = FsIo;
    let preview = asb_switch::read_codex_preview(
        &io,
        &config,
        &auth,
        &projection.plan,
        &backup_dir.to_string_lossy(),
    )
    .expect("preview isolated Codex switch");
    let committed_projection = projection.clone();
    let committed_gateway = gateway.clone();
    asb_switch::execute_codex(
        &io,
        &asb_switch::CodexSwitchRequest {
            target: &config,
            auth_target: &auth,
            plan: &projection.plan,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        move |_| committed_gateway.commit(&committed_projection, || Ok(())),
    )
    .expect("execute isolated Codex switch");
    let rendered = fs::read_to_string(&config).expect("read isolated Codex config");
    assert!(rendered.contains("web_search = \"disabled\""));
    assert!(!rendered.contains(&upstream_key));

    let mut child = Command::new("codex")
        .args([
            "exec",
            "--ephemeral",
            "--skip-git-repo-check",
            "Return exactly: sandbox answer",
        ])
        .current_dir(workdir)
        .env("CODEX_HOME", &codex_home)
        .env_remove("OPENAI_API_KEY")
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start isolated Codex CLI");
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if child.try_wait().expect("poll Codex CLI").is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("isolated Codex CLI did not finish within 45 seconds");
        }
        thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output().expect("collect Codex CLI output");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let safe_stdout = stdout
        .replace(&upstream_key, "<redacted-upstream-key>")
        .replace(
            &projection.plan.profile.api_key,
            "<redacted-loopback-token>",
        );
    let safe_stderr = stderr
        .replace(&upstream_key, "<redacted-upstream-key>")
        .replace(
            &projection.plan.profile.api_key,
            "<redacted-loopback-token>",
        );
    assert!(
        output.status.success(),
        "isolated Codex CLI must succeed; stdout={safe_stdout:?}; stderr={safe_stderr:?}"
    );
    assert!(stdout.contains("sandbox answer"));
    assert!(!stdout.contains("private sandbox reasoning"));
    assert!(!stdout.contains(&upstream_key));
    assert!(!stderr.contains(&upstream_key));

    let (authorization, body) = observed_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("one actual upstream request after local prewarm");
    assert_eq!(authorization, Some(format!("Bearer {upstream_key}")));
    assert!(!body.contains(&projection.plan.profile.api_key));
    assert!(!body.contains("previous_response_id"));
    assert!(body.contains("Return exactly: sandbox answer"));
    let upstream_body: Value = serde_json::from_str(&body).expect("Chat upstream JSON");
    let tools = upstream_body["tools"]
        .as_array()
        .expect("actual Codex request must include tools");
    assert!(tools.iter().all(|tool| tool["type"] == "function"));
    assert!(tools.iter().any(|tool| {
        tool["function"]["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("asbns_n_"))
    }));
    upstream_worker.join().expect("upstream worker");
    gateway.shutdown();
}

#[test]
#[ignore = "requires an installed Claude Code CLI and an isolated loopback sandbox"]
fn actual_claude_code_completes_through_the_isolated_gateway() {
    Command::new("claude.cmd")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("the explicit black-box test requires the Claude Code CLI");

    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let upstream_url = endpoint(&upstream);
    let upstream_key = format!("fixture-{}", uuid::Uuid::new_v4());
    let (observed_sender, observed_receiver) = mpsc::channel();
    let upstream_worker = thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(30))
            .expect("upstream receive")
            .expect("upstream request");
        let path = request.url().to_string();
        let authorization = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("Authorization"))
            .map(|header| header.value.as_str().to_string());
        let mut body = String::new();
        request
            .as_reader()
            .read_to_string(&mut body)
            .expect("read upstream request");
        let stream_requested = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| value.get("stream").and_then(Value::as_bool))
            .unwrap_or(false);
        observed_sender
            .send((path, authorization, body))
            .expect("report upstream request");
        let response = if stream_requested {
            let stream = b"data: {\"id\":\"chat_claude\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_claude\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"private sandbox reasoning\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_claude\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"sandbox answer\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_claude\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
            Response::from_data(stream.as_slice())
                .with_header(content_type("text/event-stream; charset=utf-8"))
        } else {
            Response::from_data(
                serde_json::to_vec(&json!({
                    "id": "chat_claude",
                    "object": "chat.completion",
                    "model": "sandbox-model",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "reasoning_content": "private sandbox reasoning",
                            "content": "sandbox answer"
                        },
                        "finish_reason": "stop",
                        "logprobs": null
                    }],
                    "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
                }))
                .expect("Chat response JSON"),
            )
            .with_header(content_type("application/json"))
        };
        request.respond(response).expect("respond upstream");
    });

    let directory = tempfile::tempdir().expect("temporary sandbox root");
    let state = LocalState::from_root(directory.path().join("state"));
    let record = state
        .configuration()
        .create_provider(ProviderDraft {
            app: AppKind::Claude,
            route_mode: RouteMode::Custom,
            name: "Claude Code CLI black-box sandbox".to_string(),
            base_url: Some(upstream_url),
            api_key: upstream_key.clone(),
            upstream_protocol: Some(UpstreamProtocol::ChatCompletions),
            max_output_tokens: None.into(),
            model: Some("sandbox-model".to_string()),
            model_options: None,
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        })
        .expect("create provider");
    let gateway = GatewayController::start(&state).expect("start gateway");
    let projection = gateway
        .project(&SwitchPlan::direct(
            record.profile,
            default_common_settings(AppKind::Claude),
        ))
        .expect("project route");

    let claude_config = directory.path().join("claude-config");
    let workdir = directory.path().join("work");
    let home = directory.path().join("home");
    fs::create_dir_all(&claude_config).expect("create temporary Claude config");
    fs::create_dir_all(&workdir).expect("create temporary working directory");
    fs::create_dir_all(&home).expect("create temporary home");
    let settings = claude_config.join("settings.json");
    let backup_dir = directory.path().join("backups");
    let io = FsIo;
    let preview = asb_switch::read_preview(
        &io,
        &settings,
        &projection.plan,
        &backup_dir.to_string_lossy(),
    )
    .expect("preview isolated Claude switch");
    let committed_projection = projection.clone();
    let committed_gateway = gateway.clone();
    asb_switch::execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &settings,
            plan: &projection.plan,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        move |_| committed_gateway.commit(&committed_projection, || Ok(())),
    )
    .expect("execute isolated Claude switch");
    let rendered = fs::read_to_string(&settings).expect("read isolated Claude settings");
    assert!(rendered.contains("ANTHROPIC_BASE_URL"));
    assert!(!rendered.contains(&upstream_key));
    let rendered_settings: Value =
        serde_json::from_str(&rendered).expect("isolated Claude settings JSON");
    assert_eq!(
        rendered_settings["env"]["CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS"],
        "1"
    );

    let mut child = Command::new("claude.cmd")
        .args([
            "-p",
            "--no-session-persistence",
            "--effort",
            "max",
            "--setting-sources",
            "user",
            "Return exactly: sandbox answer",
        ])
        .current_dir(workdir)
        .env("CLAUDE_CONFIG_DIR", &claude_config)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("APPDATA", home.join("AppData"))
        .env("LOCALAPPDATA", home.join("LocalAppData"))
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("ANTHROPIC_AUTH_TOKEN")
        .env_remove("ANTHROPIC_BASE_URL")
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start isolated Claude Code");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if child.try_wait().expect("poll Claude Code").is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("isolated Claude Code did not finish within 60 seconds");
        }
        thread::sleep(Duration::from_millis(50));
    }
    let output = child
        .wait_with_output()
        .expect("collect Claude Code output");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let safe_stdout = stdout
        .replace(&upstream_key, "<redacted-upstream-key>")
        .replace(
            &projection.plan.profile.api_key,
            "<redacted-loopback-token>",
        );
    let safe_stderr = stderr
        .replace(&upstream_key, "<redacted-upstream-key>")
        .replace(
            &projection.plan.profile.api_key,
            "<redacted-loopback-token>",
        );
    assert!(
        output.status.success(),
        "isolated Claude Code must succeed; stdout={safe_stdout:?}; stderr={safe_stderr:?}"
    );
    assert!(stdout.contains("sandbox answer"));
    assert!(!stdout.contains("private sandbox reasoning"));
    assert!(!stdout.contains(&upstream_key));
    assert!(!stderr.contains(&upstream_key));

    let (path, authorization, body) = observed_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("one actual upstream request");
    assert_eq!(path, "/v1/chat/completions");
    assert_eq!(authorization, Some(format!("Bearer {upstream_key}")));
    assert!(!body.contains(&projection.plan.profile.api_key));
    assert!(body.contains("Return exactly: sandbox answer"));
    let upstream_body: Value = serde_json::from_str(&body).expect("Chat upstream JSON");
    assert_eq!(upstream_body["reasoning_effort"], "max");
    upstream_worker.join().expect("upstream worker");
    gateway.shutdown();
}

#[test]
fn loopback_gateway_converts_and_isolates_anthropic_upstream_credentials() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let upstream_url = endpoint(&upstream);
    let (observed_sender, observed_receiver) = mpsc::channel();
    let upstream_worker = thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(10))
            .expect("upstream receive")
            .expect("upstream request");
        let url = request.url().to_string();
        let x_api_key = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("x-api-key"))
            .map(|header| header.value.as_str().to_string());
        let authorization = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("Authorization"))
            .map(|header| header.value.as_str().to_string());
        let anthropic_version = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("anthropic-version"))
            .map(|header| header.value.as_str().to_string());
        let mut body = String::new();
        request
            .as_reader()
            .read_to_string(&mut body)
            .expect("read upstream body");
        observed_sender
            .send((url, x_api_key, authorization, anthropic_version, body))
            .expect("report upstream request");
        let response = Response::from_data(
            serde_json::to_vec(&json!({
                "id": "msg_sandbox",
                "type": "message",
                "role": "assistant",
                "model": "sandbox-model",
                "content": [{"type": "text", "text": "converted upstream answer"}],
                "stop_reason": "end_turn",
                "stop_sequence": null,
                "usage": {"input_tokens": 3, "output_tokens": 4}
            }))
            .expect("response json"),
        )
        .with_header(content_type("application/json"));
        request.respond(response).expect("respond upstream");
    });

    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let record = state
        .configuration()
        .create_provider(ProviderDraft {
            app: AppKind::Codex,
            route_mode: RouteMode::Custom,
            name: "Anthropic sandbox".to_string(),
            base_url: Some(upstream_url),
            api_key: "upstream-x-api-key".to_string(),
            upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
            max_output_tokens: Some(8_192).into(),
            model: Some("sandbox-model".to_string()),
            model_options: None,
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        })
        .expect("create provider");
    let gateway = GatewayController::start(&state).expect("start gateway");
    let plan = SwitchPlan::direct(record.profile, default_common_settings(AppKind::Codex));
    let projection = gateway.project(&plan).expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("commit route");
    let client = Client::builder().no_proxy().build().expect("client");
    let gateway_url = format!("{}/v1/responses", gateway.inner.base_url);
    let request_body = serde_json::to_vec(&json!({
        "model": "sandbox-model",
        "input": "hello",
    }))
    .expect("request json");

    let unauthorized = client
        .post(&gateway_url)
        .header("Authorization", "Bearer incorrect-local-token")
        .header(CONTENT_TYPE, "application/json")
        .body(request_body.clone())
        .send()
        .expect("unauthorized request");
    assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED);
    let unauthorized_text = unauthorized.text().expect("unauthorized body");
    assert!(!unauthorized_text.contains("upstream-x-api-key"));
    assert!(!unauthorized_text.contains("127.0.0.1"));

    let response = client
        .post(&gateway_url)
        .header(
            "Authorization",
            format!("Bearer {}", projection.plan.profile.api_key),
        )
        .header(CONTENT_TYPE, "application/json")
        .body(request_body)
        .send()
        .expect("gateway request");
    assert!(response.status().is_success());
    let body: Value = serde_json::from_str(&response.text().expect("converted response body"))
        .expect("converted response json");
    assert_eq!(body["object"], "response");
    assert_eq!(
        body["output"][0]["content"][0]["text"],
        "converted upstream answer"
    );

    let (url, x_api_key, authorization, anthropic_version, body) = observed_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("one upstream request");
    assert_eq!(url, "/v1/messages");
    assert_eq!(x_api_key.as_deref(), Some("upstream-x-api-key"));
    assert!(authorization.is_none());
    assert_eq!(anthropic_version.as_deref(), Some("2023-06-01"));
    let upstream_body: Value = serde_json::from_str(&body).expect("anthropic request json");
    assert_eq!(upstream_body["messages"][0]["role"], "user");
    assert_eq!(upstream_body["messages"][0]["content"][0]["text"], "hello");
    assert_eq!(upstream_body["max_tokens"], 8_192);

    upstream_worker.join().expect("upstream worker");
    gateway.shutdown();
}
