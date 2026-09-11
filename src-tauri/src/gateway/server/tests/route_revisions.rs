use super::websocket_roundtrip::{
    read_websocket_text, receive_responses_completion, send_websocket_text, websocket_connect,
    websocket_request,
};
use super::*;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
struct ObservedRequest {
    tag: String,
    authorization: Option<String>,
}

struct RouteFixture {
    _directory: tempfile::TempDir,
    state: LocalState,
    gateway: GatewayController,
    first: crate::gateway::GatewayProjection,
    second: crate::gateway::GatewayProjection,
    socket: TcpStream,
    address: String,
    token: String,
    first_key: String,
    second_key: String,
    first_seen: mpsc::Receiver<ObservedRequest>,
    second_seen: mpsc::Receiver<ObservedRequest>,
    release_first_http: mpsc::Sender<()>,
    first_worker: Option<thread::JoinHandle<()>>,
    second_worker: Option<thread::JoinHandle<()>>,
}

impl RouteFixture {
    fn start() -> Self {
        let first_upstream = Server::http(("127.0.0.1", 0)).expect("first upstream listener");
        let second_upstream = Server::http(("127.0.0.1", 0)).expect("second upstream listener");
        let first_url = endpoint(&first_upstream);
        let second_url = endpoint(&second_upstream);
        let first_key = format!("route-a-{}", uuid::Uuid::new_v4());
        let second_key = format!("route-b-{}", uuid::Uuid::new_v4());
        let (first_seen_sender, first_seen) = mpsc::channel();
        let (second_seen_sender, second_seen) = mpsc::channel();
        let (release_first_http, release_first_http_receiver) = mpsc::channel();
        let first_worker = serve_first_upstream(
            first_upstream,
            first_seen_sender,
            release_first_http_receiver,
        );
        let second_worker = serve_second_upstream(second_upstream, second_seen_sender);

        let directory = tempfile::tempdir().expect("temporary state");
        let state = LocalState::from_root(directory.path().join("state"));
        let gateway = GatewayController::start(&state);
        let first_file = sandbox_codex_file(
            &state,
            "route A",
            first_url,
            first_key.clone(),
            CodexUpstream::Responses,
        );
        let second_file = sandbox_codex_file(
            &state,
            "route B",
            second_url,
            second_key.clone(),
            CodexUpstream::Responses,
        );
        let first = gateway
            .project_codex(&first_file, default_client_settings(AppKind::Codex))
            .expect("project route A");
        let second = gateway
            .project_codex(&second_file, default_client_settings(AppKind::Codex))
            .expect("project route B");
        assert_eq!(
            codex_endpoint(&first),
            codex_endpoint(&second),
            "Codex must retain one client endpoint across a route switch"
        );
        assert_eq!(
            projection_token(&first),
            projection_token(&second),
            "Codex must retain one capability across a route switch"
        );
        gateway.commit(&first, || Ok(())).expect("activate route A");
        let address = gateway
            .configured_base_url()
            .strip_prefix("http://")
            .expect("loopback HTTP address")
            .to_string();
        let token = projection_token(&first);
        let socket = websocket_connect(&address, &token);

        Self {
            _directory: directory,
            state,
            gateway,
            first,
            second,
            socket,
            address,
            token,
            first_key,
            second_key,
            first_seen,
            second_seen,
            release_first_http,
            first_worker: Some(first_worker),
            second_worker: Some(second_worker),
        }
    }

    fn complete_first_websocket_generation(&mut self) -> String {
        send_websocket_text(&mut self.socket, &websocket_prompt("ws-a", None));
        let response = receive_responses_completion(&mut self.socket);
        self.expect_first("ws-a");
        response["response"]["id"]
            .as_str()
            .expect("route A response id")
            .to_string()
    }

    fn start_inflight_first_http(&self) -> thread::JoinHandle<Value> {
        let endpoint = codex_endpoint(&self.first);
        thread::spawn(move || send_http_request(&endpoint, "http-a"))
    }

    fn activate_second_route(&self) {
        self.gateway
            .commit(&self.second, || Ok(()))
            .expect("activate route B");
    }

    fn complete_second_http_request(&self) {
        let response = send_http_request(&codex_endpoint(&self.second), "http-b");
        assert_eq!(response["id"], "resp_http_b");
        self.expect_second("http-b");
    }

    fn reject_inactive_websocket(&mut self) {
        send_websocket_text(&mut self.socket, &websocket_prompt("inactive", None));
        let event: Value = serde_json::from_str(&read_websocket_text(&mut self.socket))
            .expect("inactive route failure event");
        assert_eq!(event["type"], "response.failed");
        assert_eq!(event["response"]["error"]["code"], "route_inactive");
        assert!(
            event["response"]["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("重新连接")),
            "inactive route failure must require a fresh connection: {event}"
        );
    }

    fn reconnect_websocket(&mut self) {
        self.socket = websocket_connect(&self.address, &self.token);
    }

    fn reject_cross_route_previous_response(&mut self, previous_response_id: &str) {
        send_websocket_text(
            &mut self.socket,
            &websocket_prompt("cross-route", Some(previous_response_id)),
        );
        let event: Value = serde_json::from_str(&read_websocket_text(&mut self.socket))
            .expect("cross-route failure event");
        assert_eq!(event["type"], "response.failed");
        assert_eq!(
            event["response"]["error"]["code"],
            "previous_response_not_found"
        );
        assert!(
            event["response"]["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("full request")),
            "cross-route continuation must ask Codex to resend full context: {event}"
        );
    }

    fn complete_second_websocket_generation(&mut self) {
        send_websocket_text(&mut self.socket, &websocket_prompt("ws-b", None));
        let response = receive_responses_completion(&mut self.socket);
        assert_eq!(response["response"]["id"], "resp_ws_b");
        self.expect_second("ws-b");
    }

    fn finish(mut self, first_http: thread::JoinHandle<Value>) {
        self.release_first_http
            .send(())
            .expect("release in-flight route A request");
        let response = first_http.join().expect("route A HTTP client");
        assert_eq!(response["id"], "resp_http_a");
        assert_metric_statuses_for_projection(
            &self.gateway,
            &self.state,
            &self.first,
            &[Some(101), Some(200), Some(200)],
        );
        assert_metric_statuses_for_projection(
            &self.gateway,
            &self.state,
            &self.second,
            &[Some(101), Some(200), None, Some(200)],
        );
        self.first_worker
            .take()
            .expect("route A worker")
            .join()
            .expect("route A worker result");
        self.second_worker
            .take()
            .expect("route B worker")
            .join()
            .expect("route B worker result");
        self.gateway.shutdown();
    }

    fn expect_first(&self, tag: &str) {
        expect_observed(&self.first_seen, tag, &self.first_key);
    }

    fn expect_second(&self, tag: &str) {
        expect_observed(&self.second_seen, tag, &self.second_key);
    }
}

#[test]
fn codex_request_snapshots_survive_hot_switch_and_reject_cross_route_continuations() {
    let mut fixture = RouteFixture::start();
    let first_response_id = fixture.complete_first_websocket_generation();
    let in_flight_first_http = fixture.start_inflight_first_http();
    fixture.expect_first("http-a");

    fixture.activate_second_route();
    fixture.reject_inactive_websocket();
    fixture.reconnect_websocket();
    fixture.complete_second_http_request();
    fixture.reject_cross_route_previous_response(&first_response_id);
    fixture.complete_second_websocket_generation();
    fixture.finish(in_flight_first_http);
}

fn serve_first_upstream(
    upstream: Server,
    observed_sender: mpsc::Sender<ObservedRequest>,
    release_http: mpsc::Receiver<()>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut websocket = receive_upstream_request(&upstream);
        observed_sender
            .send(observe_request(&mut websocket))
            .expect("report route A WebSocket request");
        respond_sse(websocket, "resp_ws_a", "route A WebSocket");

        let mut http = receive_upstream_request(&upstream);
        observed_sender
            .send(observe_request(&mut http))
            .expect("report route A HTTP request");
        release_http
            .recv_timeout(REQUEST_TIMEOUT)
            .expect("release route A HTTP request");
        respond_json(http, "resp_http_a", "route A HTTP");
    })
}

fn serve_second_upstream(
    upstream: Server,
    observed_sender: mpsc::Sender<ObservedRequest>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut http = receive_upstream_request(&upstream);
        observed_sender
            .send(observe_request(&mut http))
            .expect("report route B HTTP request");
        respond_json(http, "resp_http_b", "route B HTTP");

        let mut websocket = receive_upstream_request(&upstream);
        observed_sender
            .send(observe_request(&mut websocket))
            .expect("report route B WebSocket request");
        respond_sse(websocket, "resp_ws_b", "route B WebSocket");
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
        .find(|header| header.field.equiv("Authorization"))
        .map(|header| header.value.as_str().to_string());
    let mut body = String::new();
    request
        .as_reader()
        .read_to_string(&mut body)
        .expect("read upstream body");
    let tag = ["ws-a", "http-a", "http-b", "cross-route", "ws-b"]
        .into_iter()
        .find(|tag| body.contains(*tag))
        .expect("request tag")
        .to_string();
    ObservedRequest { tag, authorization }
}

fn respond_json(request: tiny_http::Request, id: &str, text: &str) {
    let body = serde_json::to_vec(&completed_response(id, text)).expect("response JSON");
    request
        .respond(Response::from_data(body).with_header(content_type("application/json")))
        .expect("respond upstream JSON");
}

fn respond_sse(request: tiny_http::Request, id: &str, text: &str) {
    let event = json!({
        "type": "response.completed",
        "response": completed_response(id, text),
    });
    request
        .respond(
            Response::from_data(format!("data: {event}\n\n"))
                .with_header(content_type("text/event-stream")),
        )
        .expect("respond upstream SSE");
}

fn completed_response(id: &str, text: &str) -> Value {
    json!({
        "id": id,
        "object": "response",
        "status": "completed",
        "model": "sandbox-model",
        "output": [{
            "type": "message",
            "id": format!("msg_{id}"),
            "status": "completed",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": text, "annotations": [] }],
        }],
        "usage": { "input_tokens": 3, "output_tokens": 4, "total_tokens": 7 },
        "error": null,
    })
}

fn websocket_prompt(tag: &str, previous_response_id: Option<&str>) -> String {
    websocket_request(
        json!([{
            "type": "message",
            "id": format!("msg_{tag}"),
            "role": "user",
            "content": [{ "type": "input_text", "text": tag }],
        }]),
        previous_response_id,
    )
}

fn send_http_request(endpoint: &str, tag: &str) -> Value {
    let body = serde_json::to_vec(&json!({ "model": "sandbox-model", "input": tag }))
        .expect("HTTP request JSON");
    let response = Client::builder()
        .no_proxy()
        .build()
        .expect("HTTP client")
        .post(endpoint)
        .header("Authorization", "Bearer fixture-official-access")
        .header(CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .expect("gateway HTTP request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    serde_json::from_str(&response.text().expect("gateway response text"))
        .expect("gateway JSON response")
}

fn expect_observed(
    receiver: &mpsc::Receiver<ObservedRequest>,
    expected_tag: &str,
    expected_key: &str,
) {
    let request = receiver
        .recv_timeout(REQUEST_TIMEOUT)
        .expect("upstream request observation");
    assert_eq!(request.tag, expected_tag);
    let expected_authorization = format!("Bearer {expected_key}");
    assert_eq!(
        request.authorization.as_deref(),
        Some(expected_authorization.as_str())
    );
}
