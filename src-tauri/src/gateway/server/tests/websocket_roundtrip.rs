use super::*;

pub(super) fn websocket_connect(address: &str, token: &str) -> TcpStream {
    websocket_connect_with_target(address, token, "")
}

pub(super) fn websocket_connect_with_query(address: &str, token: &str, query: &str) -> TcpStream {
    websocket_connect_with_target(address, token, &format!("?{query}"))
}

fn websocket_connect_with_target(address: &str, token: &str, suffix: &str) -> TcpStream {
    let mut stream = TcpStream::connect(address).expect("connect WebSocket client");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set WebSocket read timeout");
    stream
            .write_all(
                format!(
                    "GET /codex/{token}/v1/responses{suffix} HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nAuthorization: Bearer fake-official-oauth\r\n\r\n"
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
    assert!(response
        .to_ascii_lowercase()
        .contains("sec-websocket-accept: s3pplmbitxaq9kygzzhzrbk+xoo="));
    stream
}

pub(super) fn send_websocket_text(stream: &mut TcpStream, text: &str) {
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

pub(super) fn read_websocket_text(stream: &mut TcpStream) -> String {
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

pub(super) fn websocket_request(input: Value, previous_response_id: Option<&str>) -> String {
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
        "reasoning": { "summary": "auto", "effort": "high" },
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

pub(super) fn receive_responses_completion(stream: &mut TcpStream) -> Value {
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

#[test]
fn codex_websocket_replays_visible_context_and_keeps_upstream_credentials_private() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let upstream_url = endpoint(&upstream);
    let upstream_key = format!("fixture-{}", uuid::Uuid::new_v4());
    let (observed_sender, observed_receiver) = mpsc::channel();
    let upstream_worker = serve_websocket_upstream(upstream, observed_sender);

    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let mut file = sandbox_codex_file(
        &state,
        "WebSocket chat sandbox",
        upstream_url,
        upstream_key.clone(),
        CodexUpstream::ChatCompletions,
    );
    file.profile.model_routes[0].upstream_model = "vendor-ws-model".to_string();
    let gateway = GatewayController::start(&state);
    file.parameters.settings.insert(
        "web_search".to_string(),
        SettingValue::Explicit {
            value: ConfigValue::Str("live".to_string()),
        },
    );
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("activate routed gateway");
    let address = gateway
        .configured_base_url()
        .strip_prefix("http://")
        .expect("loopback HTTP address")
        .to_string();
    let local_token = projection_token(&projection);
    let mut socket = websocket_connect(&address, &local_token);

    assert_client_completions(&mut socket, &upstream_key);
    assert_metric_statuses_for_projection(
        &gateway,
        &state,
        &projection,
        &[Some(101), Some(200), Some(200)],
    );

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
    assert_eq!(first_body["model"], "vendor-ws-model");
    assert_eq!(first_body["parallel_tool_calls"], true);
    assert_eq!(first_body["reasoning_effort"], "high");
    assert_eq!(first_body["messages"][0]["role"], "system");
    assert_eq!(first_body["messages"][1]["content"], "first question");
    let second_body: Value = serde_json::from_str(&second_body).expect("second upstream JSON");
    assert_eq!(second_body["model"], "vendor-ws-model");
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

fn serve_websocket_upstream(
    upstream: Server,
    observed_sender: mpsc::Sender<(Option<String>, String)>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
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
    })
}

fn assert_client_completions(socket: &mut TcpStream, upstream_key: &str) {
    send_websocket_text(
        socket,
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
    let first = receive_responses_completion(socket);
    assert_eq!(first["response"]["id"], "chat_ws_1");
    assert!(!first.to_string().contains(upstream_key));
    assert!(!first.to_string().contains("private reasoning 1"));

    send_websocket_text(
        socket,
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
    let second = receive_responses_completion(socket);
    assert_eq!(second["response"]["id"], "chat_ws_2");
    assert!(!second.to_string().contains(upstream_key));
    assert!(!second.to_string().contains("private reasoning 2"));
}
