use super::websocket_roundtrip::{
    read_websocket_text, receive_responses_completion, send_websocket_text, websocket_connect,
    websocket_request,
};
use super::*;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

struct ObservedRequest {
    body: Value,
    authorization: Option<String>,
}

struct RouteFixture {
    _directory: tempfile::TempDir,
    state: LocalState,
    gateway: GatewayController,
    first: crate::gateway::GatewayProjection,
    second: crate::gateway::GatewayProjection,
    second_protocol: CodexUpstream,
    socket: TcpStream,
    first_key: String,
    second_key: String,
    first_seen: mpsc::Receiver<ObservedRequest>,
    second_seen: mpsc::Receiver<ObservedRequest>,
    release_first_websocket: mpsc::Sender<()>,
    release_first_http: mpsc::Sender<()>,
    first_worker: thread::JoinHandle<()>,
    second_worker: thread::JoinHandle<()>,
}

impl RouteFixture {
    fn start(second_protocol: CodexUpstream) -> Self {
        let first_upstream = Server::http(("127.0.0.1", 0)).expect("route A listener");
        let second_upstream = Server::http(("127.0.0.1", 0)).expect("route B listener");
        let directory = tempfile::tempdir().expect("temporary state");
        let state = LocalState::from_root(directory.path().join("state"));
        let gateway = GatewayController::start(&state);
        let first_key = format!("route-a-{}", uuid::Uuid::new_v4());
        let second_key = format!("route-b-{}", uuid::Uuid::new_v4());
        let first = project_route(
            &state,
            &gateway,
            &first_upstream,
            &first_key,
            CodexUpstream::Responses,
        );
        let second = project_route(
            &state,
            &gateway,
            &second_upstream,
            &second_key,
            second_protocol,
        );
        assert_eq!(codex_endpoint(&first), codex_endpoint(&second));
        assert_eq!(projection_token(&first), projection_token(&second));
        gateway.commit(&first, || Ok(())).expect("activate route A");
        let address = gateway.configured_base_url();
        let socket = websocket_connect(
            address.strip_prefix("http://").unwrap(),
            &projection_token(&first),
        );
        let (first_seen_sender, first_seen) = mpsc::channel();
        let (second_seen_sender, second_seen) = mpsc::channel();
        let (release_first_websocket, release_websocket) = mpsc::channel();
        let (release_first_http, release_http) = mpsc::channel();
        let first_worker = serve_first_upstream(
            first_upstream,
            first_seen_sender,
            release_websocket,
            release_http,
        );
        let second_worker =
            serve_second_upstream(second_upstream, second_seen_sender, second_protocol);
        Self {
            _directory: directory,
            state,
            gateway,
            first,
            second,
            second_protocol,
            socket,
            first_key,
            second_key,
            first_seen,
            second_seen,
            release_first_websocket,
            release_first_http,
            first_worker,
            second_worker,
        }
    }

    fn prewarm(&mut self) {
        let mut request: Value = serde_json::from_str(&websocket_request(json!([]), None)).unwrap();
        request["generate"] = json!(false);
        send_websocket_text(&mut self.socket, &request.to_string());
        let response = receive_responses_completion(&mut self.socket);
        assert_eq!(response["response"]["output"], json!([]));
        assert_eq!(response["response"]["usage"]["total_tokens"], 0);
    }

    fn start_first_requests(&mut self) -> thread::JoinHandle<Value> {
        self.prewarm();
        send_websocket_text(&mut self.socket, &websocket_prompt("ws-a", None));
        expect_observed(
            &self.first_seen,
            "ws-a",
            &self.first_key,
            CodexUpstream::Responses,
        );
        let endpoint = codex_endpoint(&self.first);
        let http = thread::spawn(move || send_http_request(&endpoint, "http-a"));
        expect_observed(
            &self.first_seen,
            "http-a",
            &self.first_key,
            CodexUpstream::Responses,
        );
        http
    }

    fn switch_and_complete_first_websocket(&mut self) -> String {
        self.gateway
            .commit(&self.second, || Ok(()))
            .expect("activate route B");
        self.release_first_websocket
            .send(())
            .expect("release route A WebSocket");
        let response = receive_responses_completion(&mut self.socket);
        assert_eq!(response["response"]["id"], "resp_ws_a");
        "resp_ws_a".to_string()
    }

    fn reject_context(&mut self, request: String) {
        send_websocket_text(&mut self.socket, &request);
        let event: Value = serde_json::from_str(&read_websocket_text(&mut self.socket)).unwrap();
        assert_eq!(event["type"], "response.failed");
        assert_eq!(
            event["response"]["error"]["code"],
            "previous_response_not_found"
        );
        assert!(event["response"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("full request"));
    }

    fn reject_old_opaque_input(&mut self) {
        if self.second_protocol != CodexUpstream::Responses {
            return;
        }
        for kind in ["reasoning", "compaction", "context_compaction"] {
            self.reject_context(websocket_request(
                json!([{
                    "type":kind, "encrypted_content":"opaque-resp_ws_a", "summary":[],
                }]),
                None,
            ));
        }
    }

    fn complete_second_websocket(&mut self) {
        let mut request: Value = serde_json::from_str(&websocket_request(
            json!([
                visible_message("user", "ws-a"),
                visible_message("assistant", "route A WebSocket"),
                visible_message("user", "ws-b"),
            ]),
            None,
        ))
        .unwrap();
        request["model"] = json!("small-model");
        send_websocket_text(&mut self.socket, &request.to_string());
        let response = receive_responses_completion(&mut self.socket);
        assert_eq!(response["response"]["id"], "resp_ws_b");
        let body = expect_observed(
            &self.second_seen,
            "ws-b",
            &self.second_key,
            self.second_protocol,
        );
        assert_eq!(body["model"], "vendor-small-model");
        assert!(body.get("previous_response_id").is_none());
        assert!(!body.to_string().contains("opaque-resp_ws_a"));
        assert!(body.to_string().contains("route A WebSocket"));
        if self.second_protocol == CodexUpstream::AnthropicMessages {
            assert_eq!(
                body["max_tokens"], 2048,
                "use the current route's selected model budget"
            );
            assert_eq!(body["messages"].as_array().unwrap().len(), 3);
        } else {
            assert_eq!(body["input"].as_array().unwrap().len(), 3);
        }
    }

    fn revoke_and_assert_closed(&mut self) {
        if self.second_protocol == CodexUpstream::Responses {
            self.gateway
                .commit_activation(
                    &crate::gateway::GatewayActivation::Direct {
                        app: AppKind::Codex,
                    },
                    &[],
                    || Ok(()),
                )
                .expect("official route removes the gateway activation");
        } else {
            self.gateway
                .inner
                .routes
                .write()
                .unwrap()
                .get_mut(&AppKind::Codex)
                .unwrap()
                .client_token
                .push_str("-revoked");
        }
        send_websocket_text(&mut self.socket, &websocket_prompt("revoked", None));
        let event: Value = serde_json::from_str(&read_websocket_text(&mut self.socket)).unwrap();
        assert_eq!(event["response"]["error"]["code"], "route_inactive");
        let mut header = [0_u8; 2];
        self.socket
            .read_exact(&mut header)
            .expect("closed WebSocket frame");
        assert_eq!(
            header[0] & 0x0f,
            8,
            "inactive capability must close the socket"
        );
    }

    fn finish(self, first_http: thread::JoinHandle<Value>) {
        self.release_first_http
            .send(())
            .expect("release route A HTTP");
        assert_eq!(first_http.join().unwrap()["id"], "resp_http_a");
        assert_metric_statuses_for_projection(
            &self.gateway,
            &self.state,
            &self.first,
            &[Some(101), Some(200), Some(200), Some(200)],
        );
        let mut second_statuses = vec![None, Some(200)];
        let opaque_rejections = usize::from(self.second_protocol == CodexUpstream::Responses) * 3;
        second_statuses.extend(vec![None; opaque_rejections]);
        second_statuses.push(Some(200));
        second_statuses.extend(vec![None; opaque_rejections]);
        assert_metric_statuses_for_projection(
            &self.gateway,
            &self.state,
            &self.second,
            &second_statuses,
        );
        self.first_worker.join().expect("route A worker");
        self.second_worker.join().expect("route B worker");
        self.gateway.shutdown();
    }
}

fn verify_hot_switch(second_protocol: CodexUpstream) {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let mut fixture = RouteFixture::start(second_protocol);
    let first_http = fixture.start_first_requests();
    let first_response = fixture.switch_and_complete_first_websocket();
    fixture.reject_context(websocket_prompt("cross-route", Some(&first_response)));
    fixture.prewarm();
    fixture.reject_old_opaque_input();
    fixture.complete_second_websocket();
    fixture.reject_old_opaque_input();
    fixture.revoke_and_assert_closed();
    fixture.finish(first_http);
}

#[test]
fn codex_request_snapshots_survive_hot_switch_and_reject_cross_route_continuations() {
    verify_hot_switch(CodexUpstream::Responses);
}

#[test]
fn codex_websocket_hot_switch_uses_the_new_protocol_model_and_budget() {
    verify_hot_switch(CodexUpstream::AnthropicMessages);
}

fn project_route(
    state: &LocalState,
    gateway: &GatewayController,
    upstream: &Server,
    key: &str,
    protocol: CodexUpstream,
) -> crate::gateway::GatewayProjection {
    let name = format!("route-{}", upstream.server_addr().to_ip().unwrap().port());
    let mut file = sandbox_codex_file(state, &name, endpoint(upstream), key.into(), protocol);
    let mut model = file.profile.catalog[0].clone();
    model.id = "small-model".into();
    model.max_output_tokens = 2048;
    file.profile.catalog.push(model);
    file.profile.model_routes.push(CodexModelRoute {
        client_model: "small-model".into(),
        upstream_model: "vendor-small-model".into(),
    });
    gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .expect("project route")
}

fn serve_first_upstream(
    upstream: Server,
    observed_sender: mpsc::Sender<ObservedRequest>,
    release_websocket: mpsc::Receiver<()>,
    release_http: mpsc::Receiver<()>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut websocket = receive_upstream_request(&upstream);
        observed_sender
            .send(observe_request(&mut websocket))
            .unwrap();
        let mut http = receive_upstream_request(&upstream);
        observed_sender.send(observe_request(&mut http)).unwrap();
        release_websocket
            .recv_timeout(REQUEST_TIMEOUT)
            .expect("release route A WebSocket");
        respond_sse(
            websocket,
            "resp_ws_a",
            "route A WebSocket",
            CodexUpstream::Responses,
        );
        release_http
            .recv_timeout(REQUEST_TIMEOUT)
            .expect("release route A HTTP");
        respond_json(http, "resp_http_a", "route A HTTP");
    })
}

fn serve_second_upstream(
    upstream: Server,
    observed_sender: mpsc::Sender<ObservedRequest>,
    protocol: CodexUpstream,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut websocket = receive_upstream_request(&upstream);
        observed_sender
            .send(observe_request(&mut websocket))
            .unwrap();
        respond_sse(websocket, "resp_ws_b", "route B WebSocket", protocol);
    })
}

fn receive_upstream_request(upstream: &Server) -> tiny_http::Request {
    upstream
        .recv_timeout(REQUEST_TIMEOUT)
        .expect("upstream receive")
        .expect("upstream request")
}

fn observe_request(request: &mut tiny_http::Request) -> ObservedRequest {
    let authorization = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Authorization") || header.field.equiv("x-api-key"))
        .map(|header| header.value.as_str().to_string());
    let mut body = String::new();
    request
        .as_reader()
        .read_to_string(&mut body)
        .expect("read upstream body");
    ObservedRequest {
        body: serde_json::from_str(&body).expect("upstream request JSON"),
        authorization,
    }
}

fn respond_json(request: tiny_http::Request, id: &str, text: &str) {
    let body = completed_response(id, text);
    request
        .respond(
            Response::from_string(body.to_string()).with_header(content_type("application/json")),
        )
        .expect("respond upstream JSON");
}

fn respond_sse(request: tiny_http::Request, id: &str, text: &str, protocol: CodexUpstream) {
    let events = if protocol == CodexUpstream::Responses {
        vec![json!({"type":"response.completed", "response":completed_response(id, text)})]
    } else {
        let mut start = anthropic_response(id, "");
        start["content"] = json!([]);
        start["stop_reason"] = Value::Null;
        start["usage"]["output_tokens"] = json!(0);
        vec![
            json!({"type":"message_start", "message":start}),
            json!({"type":"content_block_start", "index":0, "content_block":{"type":"text", "text":""}}),
            json!({"type":"content_block_delta", "index":0, "delta":{"type":"text_delta", "text":text}}),
            json!({"type":"content_block_stop", "index":0}),
            json!({"type":"message_delta", "delta":{"stop_reason":"end_turn", "stop_sequence":null}, "usage":{"output_tokens":4}}),
            json!({"type":"message_stop"}),
        ]
    };
    let body = events
        .into_iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect::<String>();
    request
        .respond(Response::from_string(body).with_header(content_type("text/event-stream")))
        .expect("respond upstream SSE");
}

fn anthropic_response(id: &str, text: &str) -> Value {
    json!({"id":id, "type":"message", "role":"assistant", "model":"vendor-small-model",
        "content":[{"type":"text", "text":text}], "stop_reason":"end_turn", "stop_sequence":null,
        "usage":{"input_tokens":3, "output_tokens":4}})
}

fn completed_response(id: &str, text: &str) -> Value {
    json!({
        "id":id, "object":"response", "status":"completed", "model":"sandbox-model",
        "output":[
            {"type":"reasoning", "id":format!("rs_{id}"), "status":"completed", "summary":[],
                "encrypted_content":format!("opaque-{id}")},
            {"type":"message", "id":format!("msg_{id}"), "status":"completed", "role":"assistant",
                "content":[{"type":"output_text", "text":text, "annotations":[]}]},
        ],
        "usage":{"input_tokens":3, "output_tokens":4, "total_tokens":7}, "error":null,
    })
}

fn visible_message(role: &str, text: &str) -> Value {
    json!({"type":"message", "role":role,
        "content":[{"type":if role == "assistant" { "output_text" } else { "input_text" }, "text":text}]})
}

fn websocket_prompt(tag: &str, previous_response_id: Option<&str>) -> String {
    websocket_request(json!([visible_message("user", tag)]), previous_response_id)
}

fn send_http_request(endpoint: &str, tag: &str) -> Value {
    let response = Client::builder()
        .no_proxy()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .unwrap()
        .post(endpoint)
        .header("Authorization", "Bearer fixture-official-access")
        .header(CONTENT_TYPE, "application/json")
        .body(json!({"model":"sandbox-model", "input":tag}).to_string())
        .send()
        .expect("gateway HTTP request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    serde_json::from_str(&response.text().unwrap()).expect("gateway JSON response")
}

fn expect_observed(
    receiver: &mpsc::Receiver<ObservedRequest>,
    expected_tag: &str,
    expected_key: &str,
    protocol: CodexUpstream,
) -> Value {
    let request = receiver
        .recv_timeout(REQUEST_TIMEOUT)
        .expect("upstream observation");
    let input = request
        .body
        .get("input")
        .or_else(|| request.body.get("messages"))
        .unwrap();
    let tag = input.as_str().unwrap_or_else(|| {
        input.as_array().unwrap().last().unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
    });
    assert_eq!(tag, expected_tag);
    let expected_auth = if protocol == CodexUpstream::AnthropicMessages {
        expected_key.to_string()
    } else {
        format!("Bearer {expected_key}")
    };
    assert_eq!(request.authorization, Some(expected_auth));
    request.body
}
