mod compaction;

use super::websocket_roundtrip::{
    read_websocket_text, receive_responses_completion, send_websocket_text, websocket_connect,
};
use super::*;

const DEFAULT_MODEL: &str = "sandbox-model";
const SMALL_MODEL: &str = "small-model";
const TINY_MODEL: &str = "tiny-model";
const DEFAULT_UPSTREAM: &str = "vendor-default";

struct BudgetSandbox {
    gateway: GatewayController,
    projection: crate::gateway::GatewayProjection,
    _state: LocalState,
    _directory: tempfile::TempDir,
}

impl BudgetSandbox {
    fn new(upstream: &Server, protocol: CodexUpstream) -> Self {
        let directory = tempfile::tempdir().expect("temporary state");
        let state = LocalState::from_root(directory.path().join("state"));
        let mut file = sandbox_codex_file(
            &state,
            "model budget sandbox",
            endpoint(upstream),
            "fixture-budget-key".to_string(),
            protocol,
        );
        file.profile.model_routes[0].upstream_model = DEFAULT_UPSTREAM.to_string();
        // An upstream alias may equal another client catalog ID. Its budget
        // must still come from the requested client entry, not that other ID.
        for (id, budget) in [(SMALL_MODEL, 4_096), (TINY_MODEL, 1_024)] {
            let mut model = file.profile.catalog[0].clone();
            model.id = id.to_string();
            model.max_output_tokens = budget;
            file.profile.catalog.push(model);
            file.profile.model_routes.push(CodexModelRoute {
                client_model: id.to_string(),
                upstream_model: DEFAULT_MODEL.to_string(),
            });
        }
        let gateway = GatewayController::start(&state);
        let projection = gateway
            .project_codex(&file, default_client_settings(AppKind::Codex))
            .expect("project budget route");
        gateway
            .commit(&projection, || Ok(()))
            .expect("activate budget route");
        Self {
            gateway,
            projection,
            _state: state,
            _directory: directory,
        }
    }

    fn post(&self, operation: &str, body: &Value) -> reqwest::blocking::Response {
        Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("HTTP client")
            .post(codex_endpoint(&self.projection).replace("/responses", operation))
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .expect("gateway response")
    }

    fn socket(&self) -> TcpStream {
        let base = self.gateway.configured_base_url();
        websocket_connect(
            base.strip_prefix("http://").expect("loopback address"),
            &projection_token(&self.projection),
        )
    }
}

impl Drop for BudgetSandbox {
    fn drop(&mut self) {
        self.gateway.shutdown();
    }
}

fn budget_request(model: Option<&str>, budget: Option<Value>) -> Value {
    let mut body = json!({
        "input": [{
            "type": "message", "role": "user",
            "content": [{"type": "input_text", "text": "budget fixture"}],
        }],
    });
    if let Some(model) = model {
        body["model"] = json!(model);
    }
    if let Some(budget) = budget {
        body["max_output_tokens"] = budget;
    }
    body
}

fn websocket_budget_request(model: &str, budget: Option<Value>) -> Value {
    let mut body = budget_request(Some(model), budget);
    body["type"] = json!("response.create");
    body["store"] = json!(false);
    body["stream"] = json!(true);
    body
}

fn serve_budget_upstream(
    upstream: Server,
    protocol: UpstreamProtocol,
    count: usize,
) -> (mpsc::Receiver<Value>, thread::JoinHandle<()>) {
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        for index in 0..count {
            let mut request = upstream
                .recv_timeout(Duration::from_secs(10))
                .expect("receive upstream request")
                .expect("upstream request");
            let path = match protocol {
                asb_core::UpstreamProtocol::GeminiGenerateContent => panic!("Google native has a separate Claude fixture"),
                UpstreamProtocol::Responses => "/v1/responses",
                UpstreamProtocol::ChatCompletions => "/v1/chat/completions",
                UpstreamProtocol::AnthropicMessages => "/v1/messages",
            };
            assert_eq!(request.url(), path);
            let mut bytes = Vec::new();
            request
                .as_reader()
                .read_to_end(&mut bytes)
                .expect("request body");
            let body: Value = serde_json::from_slice(&bytes).expect("upstream JSON");
            let (media_type, response) = budget_response(protocol, &body, index);
            sender.send(body).expect("report upstream body");
            request
                .respond(Response::from_string(response).with_header(content_type(media_type)))
                .expect("respond upstream");
        }
    });
    (receiver, worker)
}

fn budget_response(
    protocol: UpstreamProtocol,
    body: &Value,
    index: usize,
) -> (&'static str, String) {
    if body["stream"] == true {
        assert_eq!(protocol, UpstreamProtocol::AnthropicMessages);
        return ("text/event-stream", anthropic_stream(&body["model"], index));
    }
    let response = match protocol {
        asb_core::UpstreamProtocol::GeminiGenerateContent => panic!("Google native has a separate Claude fixture"),
        UpstreamProtocol::AnthropicMessages => json!({
            "id": format!("msg_budget_{index}"), "type": "message", "role": "assistant",
            "model": body["model"], "content": [{"type": "text", "text": "ok"}],
            "stop_reason": "end_turn", "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1},
        }),
        UpstreamProtocol::Responses => json!({
            "id": format!("resp_budget_{index}"), "object": "response",
            "status": "completed", "model": body["model"], "output": [], "error": null,
            "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2},
        }),
        UpstreamProtocol::ChatCompletions => json!({
            "id": format!("chat_budget_{index}"), "object": "chat.completion", "created": 0,
            "model": body["model"],
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"},
                "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
        }),
    };
    ("application/json", response.to_string())
}

fn anthropic_stream(model: &Value, index: usize) -> String {
    let events = [
        json!({"type": "message_start", "message": {
            "id": format!("msg_budget_{index}"), "type": "message", "role": "assistant",
            "model": model, "content": [], "stop_reason": null, "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 0},
        }}),
        json!({"type": "content_block_start", "index": 0,
            "content_block": {"type": "text", "text": ""}}),
        json!({"type": "content_block_delta", "index": 0,
            "delta": {"type": "text_delta", "text": "ok"}}),
        json!({"type": "content_block_stop", "index": 0}),
        json!({"type": "message_delta", "delta": {
            "stop_reason": "end_turn", "stop_sequence": null,
        }, "usage": {"output_tokens": 1}}),
        json!({"type": "message_stop"}),
    ];
    events
        .iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect()
}

fn observed_body(receiver: &mpsc::Receiver<Value>) -> Value {
    receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("observed upstream request")
}

fn assert_converted_budget(body: &Value, client_model: Option<&str>, budget: u64) {
    let expected_model = if matches!(client_model, Some(SMALL_MODEL | TINY_MODEL)) {
        DEFAULT_MODEL
    } else {
        DEFAULT_UPSTREAM
    };
    assert_eq!(body["model"], expected_model);
    assert_eq!(body["max_tokens"], budget);
    assert!(body.get("max_output_tokens").is_none());
}

fn invalid_budgets() -> Vec<Value> {
    vec![
        json!(null),
        json!(false),
        json!("1024"),
        json!(0),
        json!(-1),
        json!(1.5),
        json!(4_097),
        json!(8_192),
        json!(u64::MAX),
    ]
}

#[test]
fn http_anthropic_budgets_follow_the_requested_client_catalog_entry() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let sandbox = BudgetSandbox::new(&upstream, CodexUpstream::AnthropicMessages);
    let cases = [
        (None, None, 16_384),
        (Some(DEFAULT_MODEL), None, 16_384),
        (Some(SMALL_MODEL), None, 4_096),
        (Some(SMALL_MODEL), Some(1), 1),
        (Some(SMALL_MODEL), Some(1_024), 1_024),
        (Some(SMALL_MODEL), Some(4_096), 4_096),
        (Some(DEFAULT_MODEL), Some(8_192), 8_192),
        (Some(SMALL_MODEL), None, 4_096),
    ];
    let (observed, worker) =
        serve_budget_upstream(upstream, UpstreamProtocol::AnthropicMessages, cases.len());
    for (model, explicit, expected) in cases {
        let response = sandbox.post(
            "/responses",
            &budget_request(model, explicit.map(Value::from)),
        );
        assert_eq!(response.status().as_u16(), 200, "model {model:?}");
        assert_converted_budget(&observed_body(&observed), model, expected);
    }
    worker.join().expect("upstream worker");
}

#[test]
fn websocket_anthropic_budgets_follow_the_requested_client_catalog_entry() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let sandbox = BudgetSandbox::new(&upstream, CodexUpstream::AnthropicMessages);
    let cases = [
        (DEFAULT_MODEL, None, 16_384),
        (SMALL_MODEL, None, 4_096),
        (SMALL_MODEL, Some(1_024), 1_024),
        (SMALL_MODEL, Some(4_096), 4_096),
        (DEFAULT_MODEL, Some(8_192), 8_192),
        (SMALL_MODEL, None, 4_096),
    ];
    let (observed, worker) =
        serve_budget_upstream(upstream, UpstreamProtocol::AnthropicMessages, cases.len());
    let mut socket = sandbox.socket();
    for (model, explicit, expected) in cases {
        let body = websocket_budget_request(model, explicit.map(Value::from));
        send_websocket_text(&mut socket, &body.to_string());
        let completed = receive_responses_completion(&mut socket);
        assert_eq!(completed["response"]["status"], "completed");
        assert_converted_budget(&observed_body(&observed), Some(model), expected);
    }
    drop(socket);
    worker.join().expect("upstream worker");
}

#[test]
fn http_anthropic_rejects_invalid_or_over_catalog_budgets_before_upstream() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let sandbox = BudgetSandbox::new(&upstream, CodexUpstream::AnthropicMessages);
    let (observed, worker) =
        serve_budget_upstream(upstream, UpstreamProtocol::AnthropicMessages, 1);
    for budget in invalid_budgets() {
        let body = budget_request(Some(SMALL_MODEL), Some(budget.clone()));
        let response = sandbox.post("/responses", &body);
        assert_eq!(response.status().as_u16(), 422, "budget {budget}");
        assert!(response
            .text()
            .expect("rejection body")
            .contains("max_output_tokens"));
        assert!(
            observed.try_recv().is_err(),
            "invalid budget reached upstream"
        );
    }
    let response = sandbox.post(
        "/responses",
        &budget_request(Some(SMALL_MODEL), Some(json!(4_096))),
    );
    assert_eq!(response.status().as_u16(), 200);
    assert_converted_budget(&observed_body(&observed), Some(SMALL_MODEL), 4_096);
    worker.join().expect("upstream worker");
}

#[test]
fn websocket_anthropic_rejects_invalid_or_over_catalog_budgets_before_upstream() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let sandbox = BudgetSandbox::new(&upstream, CodexUpstream::AnthropicMessages);
    let (observed, worker) =
        serve_budget_upstream(upstream, UpstreamProtocol::AnthropicMessages, 1);
    let mut socket = sandbox.socket();
    for budget in invalid_budgets() {
        let body = websocket_budget_request(SMALL_MODEL, Some(budget));
        send_websocket_text(&mut socket, &body.to_string());
        let failed: Value =
            serde_json::from_str(&read_websocket_text(&mut socket)).expect("failure JSON");
        assert_eq!(failed["type"], "response.failed");
        assert_eq!(failed["response"]["error"]["code"], "invalid_request");
        assert!(failed.to_string().contains("max_output_tokens"));
        assert!(
            observed.try_recv().is_err(),
            "invalid budget reached upstream"
        );
    }
    let body = websocket_budget_request(SMALL_MODEL, Some(json!(4_096)));
    send_websocket_text(&mut socket, &body.to_string());
    receive_responses_completion(&mut socket);
    assert_converted_budget(&observed_body(&observed), Some(SMALL_MODEL), 4_096);
    drop(socket);
    worker.join().expect("upstream worker");
}

#[test]
fn native_responses_and_chat_preserve_their_original_budget_fields() {
    for protocol in [CodexUpstream::Responses, CodexUpstream::ChatCompletions] {
        let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
        let sandbox = BudgetSandbox::new(&upstream, protocol);
        let (observed, worker) = serve_budget_upstream(upstream, protocol.protocol(), 2);
        for explicit in [false, true] {
            let (operation, mut body) = match protocol {
                CodexUpstream::Responses => (
                    "/responses",
                    budget_request(Some(SMALL_MODEL), explicit.then(|| json!(8_192))),
                ),
                CodexUpstream::ChatCompletions => {
                    let mut body = json!({"model": SMALL_MODEL, "messages": [{"role": "user", "content": "fixture"}]});
                    if explicit {
                        body["max_tokens"] = json!(1_024);
                        body["max_completion_tokens"] = json!(8_192);
                    }
                    ("/chat/completions", body)
                }
                CodexUpstream::AnthropicMessages => unreachable!(),
            };
            let response = sandbox.post(operation, &body);
            assert_eq!(response.status().as_u16(), 200);
            body["model"] = json!(DEFAULT_MODEL);
            assert_eq!(observed_body(&observed), body);
        }
        worker.join().expect("upstream worker");
    }
}
