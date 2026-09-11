//! Opt-in external verification of the specialized Codex gateway contract.
//!
//! Each route uses a fresh temporary application state, temporary Codex
//! configuration and its own gateway controller. The provider credential comes
//! only from the process environment.

use super::live_support::{
    credential, failure_summary, prepare_live_sandbox, LiveRoute, LIVE_ROUTES, MODEL,
};
use reqwest::blocking::Client;
use reqwest::header::CONTENT_TYPE;
use serde_json::{json, Value};
use std::time::Duration;

#[test]
#[ignore = "requires ASB_CODEX_LIVE_API_KEY and three protocol-specific upstream URLs"]
fn external_provider_completes_through_all_codex_protocols() {
    let api_key = credential();
    for route in &LIVE_ROUTES {
        run_live_route(route, &api_key);
    }
}

fn run_live_route(route: &LiveRoute, api_key: &str) {
    let sandbox = prepare_live_sandbox(route, api_key);
    let expected = format!("ASB_LIVE_CODEX_{}_OK", route.label.to_ascii_uppercase());
    let follow_up = format!("{expected}_REPLY");
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(90))
        .build()
        .expect("external HTTP client");
    let base_url = sandbox.client_base_url();

    let first = read_live_body(
        send_live_request(&client, &base_url, &expected, false, route),
        route,
        api_key,
    );
    let first: Value = serde_json::from_str(&first).unwrap_or_else(|_| {
        panic!(
            "{} non-stream response was not Codex Responses JSON",
            route.label
        )
    });
    assert_response_marker(&first, &expected, route.label, "non-stream");

    // A real Codex client replays the previous output items, including the
    // opaque reasoning item, on the next turn of the same conversation.
    let replayed = first["output"]
        .as_array()
        .expect("first response output")
        .to_vec();
    assert_non_stream_response(
        send_follow_up_request(&client, &base_url, &replayed, &follow_up, route),
        route,
        api_key,
        &follow_up,
    );

    assert_stream_response(
        send_live_request(&client, &base_url, &expected, true, route),
        route,
        api_key,
        &expected,
    );
    sandbox.shutdown();
}

fn send_follow_up_request(
    client: &Client,
    client_base_url: &str,
    replayed: &[Value],
    expected: &str,
    route: &LiveRoute,
) -> reqwest::blocking::Response {
    let mut input = replayed.to_vec();
    input.push(json!({
        "type": "message",
        "role": "user",
        "content": [{ "type": "input_text", "text": format!("Reply with exactly {expected} and no other text.") }],
    }));
    client
        .post(format!("{client_base_url}/responses"))
        .header(CONTENT_TYPE, "application/json")
        .header("Authorization", "Bearer isolated-official-token")
        .header("openai-beta", "responses=v1")
        .body(
            json!({
                "model": MODEL,
                "input": input,
                "max_output_tokens": 512,
                "stream": false,
            })
            .to_string(),
        )
        .send()
        .unwrap_or_else(|error| panic!("{} follow-up request failed: {error}", route.label))
}

fn send_live_request(
    client: &Client,
    client_base_url: &str,
    expected: &str,
    stream: bool,
    route: &LiveRoute,
) -> reqwest::blocking::Response {
    client
        .post(format!("{client_base_url}/responses"))
        .header(CONTENT_TYPE, "application/json")
        .header("Authorization", "Bearer isolated-official-token")
        .header("openai-beta", "responses=v1")
        .body(
            json!({
                "model": MODEL,
                "input": format!("Reply with exactly {expected} and no other text."),
                "max_output_tokens": 512,
                "stream": stream,
            })
            .to_string(),
        )
        .send()
        .unwrap_or_else(|error| panic!("{} upstream request failed: {error}", route.label))
}

fn assert_non_stream_response(
    response: reqwest::blocking::Response,
    route: &LiveRoute,
    api_key: &str,
    expected: &str,
) {
    let body = read_live_body(response, route, api_key);
    let response: Value = serde_json::from_str(&body).unwrap_or_else(|_| {
        panic!(
            "{} non-stream response was not Codex Responses JSON",
            route.label
        )
    });
    assert_response_marker(&response, expected, route.label, "non-stream");
}

fn assert_stream_response(
    response: reqwest::blocking::Response,
    route: &LiveRoute,
    api_key: &str,
    expected: &str,
) {
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        content_type.starts_with("text/event-stream"),
        "{} stream did not return an SSE content type",
        route.label
    );
    let body = response.text().unwrap_or_else(|error| {
        panic!("{} stream response could not be read: {error}", route.label)
    });
    assert!(
        !body.contains(api_key),
        "{} stream response echoed the provider credential",
        route.label
    );
    assert!(
        status.is_success(),
        "{} stream request returned HTTP {}: {}",
        route.label,
        status,
        failure_summary(&body, api_key)
    );
    let response = stream_completed_response(&body).unwrap_or_else(|| {
        panic!(
            "{} stream had no response.completed event: {}",
            route.label,
            failure_summary(&body, api_key)
        )
    });
    assert_response_marker(&response, expected, route.label, "stream");
}

fn read_live_body(
    response: reqwest::blocking::Response,
    route: &LiveRoute,
    api_key: &str,
) -> String {
    let status = response.status();
    let body = response.text().unwrap_or_else(|error| {
        panic!(
            "{} non-stream response could not be read: {error}",
            route.label
        )
    });
    assert!(
        !body.contains(api_key),
        "{} non-stream response echoed the provider credential",
        route.label
    );
    assert!(
        status.is_success(),
        "{} non-stream request returned HTTP {}: {}",
        route.label,
        status,
        failure_summary(&body, api_key)
    );
    body
}

fn stream_completed_response(body: &str) -> Option<Value> {
    body.replace("\r\n", "\n")
        .split("\n\n")
        .filter_map(|event| {
            event
                .lines()
                .find_map(|line| line.strip_prefix("data:").map(str::trim))
                .and_then(|data| serde_json::from_str::<Value>(data).ok())
        })
        .find_map(|event| {
            (event["type"] == "response.completed").then(|| event["response"].clone())
        })
}

fn assert_response_marker(response: &Value, expected: &str, label: &str, mode: &str) {
    assert_eq!(
        response["object"], "response",
        "{label} {mode} response shape"
    );
    assert!(
        response["output"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|item| item["content"].as_array().into_iter().flatten())
            .filter_map(|part| part["text"].as_str())
            .any(|text| text.contains(expected)),
        "{label} {mode} response did not contain its requested completion marker"
    );
}

#[test]
fn stream_completed_response_extracts_a_terminal_responses_payload() {
    let body = concat!(
        "event: response.created\r\n",
        "data: {\"type\":\"response.created\"}\r\n\r\n",
        "event: response.completed\r\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"object\":\"response\",\"output\":[]}}\r\n\r\n"
    );
    let response = stream_completed_response(body).expect("terminal response event");
    assert_eq!(response["object"], "response");
}

#[test]
fn failure_summary_redacts_the_live_credential() {
    let credential = "sk-live-test-credential";
    let summary = failure_summary(
        &json!({"error": {"message": format!("credential={credential}")}}).to_string(),
        credential,
    );
    assert!(!summary.contains(credential));
}
