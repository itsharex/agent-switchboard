use super::*;

const ROUTE_A: &str = "route-a";
const ROUTE_B: &str = "route-b";

fn request(input: Value, previous_response_id: Option<&str>, session_id: &str) -> String {
    let mut value = json!({
        "type": "response.create",
        "model": "sandbox-model",
        "input": input,
        "store": false,
        "stream": true,
        "client_metadata": { "session_id": session_id },
    });
    if let Some(previous_response_id) = previous_response_id {
        value["previous_response_id"] = json!(previous_response_id);
    }
    value.to_string()
}

fn message(role: &str, text: &str) -> Value {
    json!({
        "type": "message",
        "role": role,
        "content": [{ "type": "input_text", "text": text }],
    })
}

fn completed(id: &str) -> Value {
    json!({
        "id": id,
        "object": "response",
        "status": "completed",
        "model": "sandbox-model",
        "output": [{
            "type": "message",
            "id": format!("message-{id}"),
            "status": "completed",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "answer", "annotations": [] }],
        }],
        "usage": { "input_tokens": 1, "output_tokens": 1, "total_tokens": 2 },
        "error": null,
    })
}

fn completed_with_reasoning(id: &str) -> Value {
    let mut response = completed(id);
    let message = response["output"][0].clone();
    response["output"] = json!([
        {
            "type": "reasoning",
            "id": format!("reasoning-{id}"),
            "status": "completed",
            "summary": [],
            "content": [],
            "encrypted_content": "asb-reasoning-v1.route-a",
        },
        message,
    ]);
    response
}

fn pending(
    context: &mut ConversationContext,
    route_revision: &str,
    request: String,
) -> PendingRequest {
    match context
        .prepare(route_revision, ResponsesRequestMode::Standard, &request)
        .expect("prepare Responses request")
    {
        PreparedResponse::Upstream(request) => request,
        PreparedResponse::Prewarm(_) => panic!("test requires an upstream request"),
    }
}

#[test]
fn rejects_incremental_context_when_route_or_session_changes() {
    let mut context = ConversationContext::default();
    let first = pending(
        &mut context,
        ROUTE_A,
        request(json!([message("user", "first")]), None, "session-a"),
    );
    context
        .record_completed(&first, &completed("response-a"))
        .expect("record route A response");

    for (route_revision, session_id) in [(ROUTE_B, "session-a"), (ROUTE_A, "session-b")] {
        let error = context
            .prepare(
                route_revision,
                ResponsesRequestMode::Standard,
                &request(json!([]), Some("response-a"), session_id),
            )
            .expect_err("continuation must retain its route and session binding");
        assert_eq!(error.code, "previous_response_not_found");
        assert!(error.message.contains("full request"));
    }
}

#[test]
fn accepts_full_visible_history_after_switch_and_rebinds_the_cache() {
    let mut context = ConversationContext::default();
    let first = pending(
        &mut context,
        ROUTE_A,
        request(json!([message("user", "first")]), None, "session-a"),
    );
    context
        .record_completed(&first, &completed("response-a"))
        .expect("record route A response");

    let second = pending(
        &mut context,
        ROUTE_B,
        request(
            json!([
                message("user", "first"),
                message("assistant", "answer"),
                message("user", "continue"),
            ]),
            None,
            "session-a",
        ),
    );
    let body: Value = serde_json::from_slice(&second.body).expect("route B request JSON");
    assert_eq!(body["input"].as_array().map(Vec::len), Some(3));
    context
        .record_completed(&second, &completed("response-b"))
        .expect("record route B response");

    let continued = pending(
        &mut context,
        ROUTE_B,
        request(
            json!([message("user", "next")]),
            Some("response-b"),
            "session-a",
        ),
    );
    let body: Value = serde_json::from_slice(&continued.body).expect("continued route B JSON");
    assert_eq!(body["input"][0]["content"][0]["text"], "first");
    assert_eq!(
        body["input"]
            .as_array()
            .expect("continued input")
            .last()
            .unwrap()["content"][0]["text"],
        "next"
    );
}

#[test]
fn rejects_opaque_continuation_from_the_previous_route_without_a_response_id() {
    let mut context = ConversationContext::default();
    let first = pending(
        &mut context,
        ROUTE_A,
        request(json!([message("user", "first")]), None, "session-a"),
    );
    context
        .record_completed(&first, &completed_with_reasoning("response-a"))
        .expect("record route A opaque continuation");

    let error = context
        .prepare(
            ROUTE_B,
            ResponsesRequestMode::Standard,
            &request(
                json!([{
                    "type": "reasoning",
                    "id": "reasoning-response-a",
                    "status": "completed",
                    "summary": [],
                    "content": [],
                    "encrypted_content": "asb-reasoning-v1.route-a",
                }]),
                None,
                "session-a",
            ),
        )
        .expect_err("opaque state cannot cross a route switch");
    assert_eq!(error.code, "previous_response_not_found");
    assert!(error.message.contains("without opaque continuation state"));
}

#[test]
fn stale_native_input_is_rejected_before_cross_protocol_normalization() {
    let mut context = ConversationContext::for_route(ROUTE_A);
    let first = pending(
        &mut context,
        ROUTE_A,
        request(json!([message("user", "first")]), None, "session-a"),
    );
    context
        .record_completed(&first, &completed("response-a"))
        .unwrap();
    for previous in [Some("response-a"), None] {
        let error = context
            .prepare(
                ROUTE_B,
                ResponsesRequestMode::Standard,
                &request(
                    json!([{"type":"context_compaction", "encrypted_content":"opaque-a"}]),
                    previous,
                    "session-a",
                ),
            )
            .expect_err("old native state must ask for visible context, not a new protocol shape");
        assert_eq!(error.code, "previous_response_not_found");
        assert!(error.message.contains("full request"));
    }
    let retry = pending(
        &mut context,
        ROUTE_B,
        request(json!([message("user", "visible retry")]), None, "session-a"),
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&retry.body).unwrap()["input"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
