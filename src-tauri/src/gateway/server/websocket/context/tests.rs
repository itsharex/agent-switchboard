use super::*;

fn request(input: Value, previous_response_id: Option<&str>) -> String {
    let mut value = json!({
        "type": "response.create",
        "model": "sandbox-model",
        "instructions": "system",
        "input": input,
        "tools": [{ "type": "function", "name": "shell", "parameters": { "type": "object" }, "strict": false }],
        "tool_choice": "auto",
        "parallel_tool_calls": true,
        "reasoning": { "summary": "auto" },
        "store": false,
        "stream": true,
        "include": ["reasoning.encrypted_content"],
        "prompt_cache_key": "turn-a",
        "client_metadata": { "session_id": "session-a" },
        "generate": true,
    });
    if let Some(previous_response_id) = previous_response_id {
        value["previous_response_id"] = Value::String(previous_response_id.to_string());
    }
    value.to_string()
}

fn completed(id: &str) -> Value {
    json!({
        "id": id,
        "object": "response",
        "status": "completed",
        "model": "sandbox-model",
        "output": [{
            "type": "message",
            "id": "msg-a",
            "status": "completed",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "first", "annotations": [] }],
        }],
        "usage": { "input_tokens": 1, "output_tokens": 1, "total_tokens": 2 },
        "error": null,
    })
}

#[test]
fn reconstructs_the_visible_previous_response_for_an_incremental_request() {
    let first_input = json!([{
        "type": "message",
        "id": "msg-user-a",
        "role": "user",
        "content": [{ "type": "input_text", "text": "first" }],
        "internal_chat_message_metadata_passthrough": { "turn_id": "turn-a" },
    }]);
    let mut context = ConversationContext::default();
    let first = match context.prepare(&request(first_input, None)).unwrap() {
        PreparedResponse::Upstream(request) => request,
        PreparedResponse::Prewarm(_) => panic!("must prepare upstream request"),
    };
    let first_body: Value = serde_json::from_slice(&first.body).unwrap();
    assert!(first_body.get("client_metadata").is_none());
    assert!(first_body.get("reasoning").is_none());
    assert_eq!(first_body["input"][0]["content"][0]["text"], "first");
    context
        .record_completed(&first, &completed("resp-a"))
        .unwrap();

    let second_input = json!([{
        "type": "message",
        "id": "msg-user-b",
        "role": "user",
        "content": [{ "type": "input_text", "text": "second" }],
    }]);
    let second = match context
        .prepare(&request(second_input, Some("resp-a")))
        .unwrap()
    {
        PreparedResponse::Upstream(request) => request,
        PreparedResponse::Prewarm(_) => panic!("must prepare upstream request"),
    };
    let body: Value = serde_json::from_slice(&second.body).unwrap();
    assert_eq!(body["input"].as_array().unwrap().len(), 3);
    assert_eq!(body["input"][1]["role"], "assistant");
    assert_eq!(body["input"][2]["content"][0]["text"], "second");
}

#[test]
fn replays_a_completed_namespaced_function_call_without_losing_its_namespace() {
    let first_input = json!([{
        "type": "message",
        "role": "user",
        "content": [{ "type": "input_text", "text": "first" }],
    }]);
    let mut context = ConversationContext::default();
    let first = match context
        .prepare(&request(first_input, None))
        .expect("first request")
    {
        PreparedResponse::Upstream(request) => request,
        PreparedResponse::Prewarm(_) => panic!("must prepare upstream request"),
    };
    context
        .record_completed(
            &first,
            &json!({
                "id": "resp-call",
                "object": "response",
                "status": "completed",
                "model": "sandbox-model",
                "output": [{
                    "type": "function_call",
                    "id": "fc_1",
                    "call_id": "call_1",
                    "name": "apply.patch",
                    "namespace": "functions.",
                    "arguments": "{\"path\":\"a\"}",
                    "status": "completed"
                }],
                "usage": { "input_tokens": 1, "output_tokens": 1, "total_tokens": 2 },
                "error": null
            }),
        )
        .expect("record function call");

    let second = match context
        .prepare(&request(json!([]), Some("resp-call")))
        .expect("incremental request")
    {
        PreparedResponse::Upstream(request) => request,
        PreparedResponse::Prewarm(_) => panic!("must prepare upstream request"),
    };
    let body: Value = serde_json::from_slice(&second.body).expect("upstream JSON");
    assert_eq!(body["input"][1]["name"], "apply.patch");
    assert_eq!(body["input"][1]["namespace"], "functions.");
}

#[test]
fn missing_previous_response_tells_codex_to_retry_the_full_request() {
    let mut context = ConversationContext::default();
    let error = context
        .prepare(&request(json!([]), Some("missing")))
        .expect_err("missing response must fail");
    assert_eq!(error.code, "previous_response_not_found");
}

#[test]
fn omitted_generate_uses_the_codex_wire_default_of_a_real_response() {
    let mut value: Value = serde_json::from_str(&request(json!([]), None)).unwrap();
    value.as_object_mut().unwrap().remove("generate");
    assert!(matches!(
        ConversationContext::default().prepare(&value.to_string()),
        Ok(PreparedResponse::Upstream(_))
    ));
}

#[test]
fn rejects_nonautomatic_reasoning_before_an_upstream_request() {
    let mut value: Value = serde_json::from_str(&request(json!([]), None)).unwrap();
    value["reasoning"] = json!({ "effort": "high" });
    let error = ConversationContext::default()
        .prepare(&value.to_string())
        .expect_err("unsupported reasoning must fail");
    assert_eq!(error.code, "invalid_request");
}

#[test]
fn replays_gateway_owned_reasoning_for_an_incremental_request() {
    let mut context = ConversationContext::default();
    let first = match context
        .prepare(&request(
            json!([{
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": "first" }],
            }]),
            None,
        ))
        .expect("first request")
    {
        PreparedResponse::Upstream(request) => request,
        PreparedResponse::Prewarm(_) => panic!("must prepare upstream request"),
    };
    context
            .record_completed(
                &first,
                &json!({
                    "id": "resp-reasoning",
                    "object": "response",
                    "status": "completed",
                    "model": "sandbox-model",
                    "output": [
                        {
                            "type": "reasoning",
                            "id": "rs_resp-reasoning_0",
                            "status": "completed",
                            "summary": [],
                            "content": [],
                            "encrypted_content": "asb-reasoning-v1.opaque"
                        },
                        {
                            "type": "message",
                            "id": "msg-reasoning",
                            "status": "completed",
                            "role": "assistant",
                            "content": [{ "type": "output_text", "text": "first answer", "annotations": [] }]
                        }
                    ],
                    "usage": { "input_tokens": 1, "output_tokens": 1, "total_tokens": 2 },
                    "error": null
                }),
            )
            .expect("record opaque reasoning");

    let second = match context
        .prepare(&request(json!([]), Some("resp-reasoning")))
        .expect("incremental request")
    {
        PreparedResponse::Upstream(request) => request,
        PreparedResponse::Prewarm(_) => panic!("must prepare upstream request"),
    };
    let body: Value = serde_json::from_slice(&second.body).expect("upstream JSON");
    assert_eq!(body["input"][1]["type"], "reasoning");
    assert_eq!(
        body["input"][1]["encrypted_content"],
        "asb-reasoning-v1.opaque"
    );
}
