use super::websocket_roundtrip::{
    read_websocket_text, send_websocket_text, websocket_connect,
};
use super::*;
use crate::codex_metering::{
    self, CodexBilling, CodexLedgerFilter, CodexMeteringSettings, CodexModelPrice,
    CodexRequestLedger,
};

struct Fixture {
    _paths: crate::test_client_paths::ClientPathGuard,
    _directory: tempfile::TempDir,
    state: LocalState,
    gateway: GatewayController,
    file: CodexProviderFile,
    projection: crate::gateway::GatewayProjection,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.gateway.shutdown();
    }
}
impl Fixture {
    fn new(server: &Server, protocol: CodexUpstream, limit: Option<&str>) -> Self {
        let paths = crate::test_client_paths::redirect_client_paths();
        let directory = tempfile::tempdir().unwrap();
        let state = LocalState::from_root(directory.path().join("state"));
        let mut file = sandbox_codex_file(
            &state,
            "Codex metering",
            endpoint(server),
            "fixture-metering-key".into(),
            protocol,
        );
        file.profile.model_routes[0].upstream_model = "wire-model".into();
        let mut settings = CodexMeteringSettings::default();
        settings.prices.insert(
            "wire-model".into(),
            CodexModelPrice {
                input_usd_per_million: "1".into(),
                output_usd_per_million: "2".into(),
                cache_read_usd_per_million: "0.1".into(),
                cache_creation_usd_per_million: "1.25".into(),
                source: "loopback fixture".into(),
            },
        );
        settings.providers.insert(
            file.profile.id.clone(),
            CodexBilling {
                daily_limit_usd: limit.map(str::to_string),
                ..Default::default()
            },
        );
        let initial = codex_metering::read_settings(state.root()).unwrap();
        codex_metering::save_settings(state.root(), settings, &initial.revision).unwrap();
        let gateway = GatewayController::start(&state);
        let projection = gateway
            .project_codex(&file, default_client_settings(AppKind::Codex))
            .unwrap();
        gateway.commit(&projection, || Ok(())).unwrap();
        Self {
            _paths: paths,
            _directory: directory,
            state,
            gateway,
            file,
            projection,
        }
    }
    fn post(&self, body: Value) -> reqwest::blocking::Response {
        Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
            .post(codex_endpoint(&self.projection))
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .unwrap()
    }
    fn ledger(&self) -> CodexRequestLedger {
        CodexRequestLedger::new(self.state.root())
    }
}
fn answer(protocol: CodexUpstream) -> Value {
    match protocol {
        CodexUpstream::Responses => {
            json!({"id":"reply","object":"response","status":"completed","model":"echoed-model",
            "output":[{"id":"message","type":"message","role":"assistant","status":"completed",
                "content":[{"type":"output_text","text":"hello","annotations":[]}]}],
            "usage":{"input_tokens":10,"output_tokens":3,"input_tokens_details":{"cached_tokens":4}}})
        }
        CodexUpstream::ChatCompletions => json!({"id":"reply","model":"echoed-model",
            "choices":[{"index":0,"message":{"role":"assistant","content":"hello"},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":10,"completion_tokens":3,"prompt_tokens_details":{"cached_tokens":4}}}),
        CodexUpstream::AnthropicMessages => {
            json!({"id":"reply","type":"message","role":"assistant","model":"echoed-model",
            "content":[{"type":"text","text":"hello"}],"stop_reason":"end_turn",
            "usage":{"input_tokens":6,"cache_read_input_tokens":4,"output_tokens":3}})
        }
    }
}
fn request_body(stream: bool) -> Value {
    json!({"model":"sandbox-model","input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}],"stream":stream})
}
fn serve_json(server: Server, protocol: CodexUpstream) -> thread::JoinHandle<Value> {
    thread::spawn(move || {
        let mut request = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        let mut text = String::new();
        request.as_reader().read_to_string(&mut text).unwrap();
        let body: Value = serde_json::from_str(&text).unwrap();
        let (content, mime) = if body["stream"] == true {
            (stream_answer(protocol), "text/event-stream")
        } else {
            (answer(protocol).to_string(), "application/json")
        };
        request
            .respond(Response::from_string(content).with_header(content_type(mime)))
            .unwrap();
        serde_json::from_str(&text).unwrap()
    })
}
#[test]
fn codex_http_and_websocket_records_survive_restart_for_all_three_protocols() {
    for protocol in [
        CodexUpstream::Responses,
        CodexUpstream::ChatCompletions,
        CodexUpstream::AnthropicMessages,
    ] {
        for websocket in [false, true] {
            exercise_json(protocol, websocket);
        }
    }
}
fn exercise_json(protocol: CodexUpstream, websocket: bool) {
    let server = Server::http(("127.0.0.1", 0)).unwrap();
    let fixture = Fixture::new(&server, protocol, None);
    let worker = serve_json(server, protocol);
    if websocket {
        let base = fixture.gateway.configured_base_url();
        let mut socket = websocket_connect(
            base.strip_prefix("http://").unwrap(),
            &projection_token(&fixture.projection),
        );
        let mut body = request_body(true);
        body["type"] = json!("response.create");
        body["store"] = json!(false);
        send_websocket_text(&mut socket, &body.to_string());
        completed(&mut socket, protocol);
        drop(socket);
    } else {
        assert_eq!(fixture.post(request_body(false)).status().as_u16(), 200);
    }
    assert_eq!(worker.join().unwrap()["model"], "wire-model");
    let deadline = Instant::now() + Duration::from_secs(3);
    while fixture.gateway.inner.inflight.load(Ordering::Acquire) != 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fixture.gateway.inner.inflight.load(Ordering::Acquire), 0);
    fixture.gateway.shutdown();
    let page = fixture.ledger().page(&Default::default(), 0, 10).unwrap();
    assert_eq!(page.total, 1);
    let record = &page.records[0];
    assert_eq!(record.profile_id.as_ref(), Some(&fixture.file.profile.id));
    assert_eq!(record.request_model.as_deref(), Some("sandbox-model"));
    assert_eq!(record.mapped_model.as_deref(), Some("wire-model"));
    assert_eq!(record.response_model.as_deref(), Some("echoed-model"));
    assert_eq!(
        (
            record.input_tokens,
            record.output_tokens,
            record.cache_read_tokens
        ),
        (Some(10), Some(3), Some(4))
    );
    assert_eq!(record.cost.as_ref().unwrap().total_usd, "0.000012");
    assert_eq!(record.attempts.len(), 1);
    assert!(record.billable);
    assert!(!serde_json::to_string(record)
        .unwrap()
        .contains("fixture-metering-key"));
}
#[test]
fn local_codex_limit_stops_the_next_request_without_penalizing_health() {
    let server = Server::http(("127.0.0.1", 0)).unwrap();
    let fixture = Fixture::new(&server, CodexUpstream::ChatCompletions, Some("0.000001"));
    let worker = serve_json(server, CodexUpstream::ChatCompletions);
    assert_eq!(fixture.post(request_body(false)).status().as_u16(), 200);
    worker.join().unwrap();
    let response = fixture.post(request_body(false));
    assert_eq!(response.status().as_u16(), 429);
    assert!(response.text().unwrap().contains("本地日限额"));
    let health = fixture
        .gateway
        .codex_endpoint_health(&fixture.file, &Default::default())
        .unwrap();
    assert!(serde_json::to_value(health)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .all(|health| health["consecutiveFailures"] == 0));
    let page = fixture
        .ledger()
        .page(&CodexLedgerFilter::default(), 0, 10)
        .unwrap();
    assert_eq!(page.total, 2);
    assert!(!page.records[0].billable);
    assert!(page.records[0].attempts.is_empty());
}
#[test]
fn codex_native_sse_records_usage_and_latency_without_persisting_messages() {
    let server = Server::http(("127.0.0.1", 0)).unwrap();
    let fixture = Fixture::new(&server, CodexUpstream::Responses, None);
    let worker = thread::spawn(move || {
        let request = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        let events = [
            json!({"type":"response.output_text.delta","delta":"private-prompt-answer"}),
            json!({"type":"response.completed","response":answer(CodexUpstream::Responses)}),
        ];
        let content = events
            .iter()
            .map(|event| {
                format!(
                    "event: {}\ndata: {event}\n\n",
                    event["type"].as_str().unwrap()
                )
            })
            .collect::<String>();
        request
            .respond(Response::from_string(content).with_header(content_type("text/event-stream")))
            .unwrap();
    });
    let response = fixture.post(request_body(true));
    assert_eq!(response.status().as_u16(), 200);
    assert!(response.text().unwrap().contains("response.completed"));
    worker.join().unwrap();
    fixture.gateway.shutdown();
    let record = fixture
        .ledger()
        .page(&Default::default(), 0, 10)
        .unwrap()
        .records
        .remove(0);
    assert_eq!(record.input_tokens, Some(10));
    assert!(record.first_byte_latency_ms.is_some());
    assert!(record.first_token_latency_ms.is_some());
    assert_eq!(record.status, Some(200));
    assert!(!serde_json::to_string(&record)
        .unwrap()
        .contains("private-prompt-answer"));
}
fn stream_answer(protocol: CodexUpstream) -> String {
    let events = match protocol {
        CodexUpstream::Responses => vec![
            json!({"type":"response.output_text.delta","delta":"hello"}),
            json!({"type":"response.completed","response":answer(protocol)}),
        ],
        CodexUpstream::ChatCompletions => vec![
            json!({"id":"reply","model":"echoed-model","choices":[{"index":0,
                "delta":{"role":"assistant","content":"hello"},"finish_reason":null}]}),
            json!({"id":"reply","model":"echoed-model","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],
                "usage":{"prompt_tokens":10,"completion_tokens":3,"prompt_tokens_details":{"cached_tokens":4}}}),
        ],
        CodexUpstream::AnthropicMessages => vec![
            json!({"type":"message_start","message":{"id":"reply","type":"message","role":"assistant",
                "model":"echoed-model","content":[],"stop_reason":null,"usage":{"input_tokens":6,"cache_read_input_tokens":4,"output_tokens":0}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}),
            json!({"type":"message_stop"}),
        ],
    };
    let mut content = events
        .iter()
        .map(|event| {
            if let Some(kind) = event["type"].as_str() {
                format!("event: {kind}\ndata: {event}\n\n")
            } else {
                format!("data: {event}\n\n")
            }
        })
        .collect::<String>();
    if protocol == CodexUpstream::ChatCompletions {
        content.push_str("data: [DONE]\n\n");
    }
    content
}

fn completed(socket: &mut TcpStream, protocol: CodexUpstream) {
    for _ in 0..32 {
        let value: Value = serde_json::from_str(&read_websocket_text(socket)).unwrap();
        assert_ne!(value["type"],"response.failed","{protocol:?}: {value}");
        if value["type"] == "response.completed" { return; }
    }
    panic!("missing completion for {protocol:?}");
}
