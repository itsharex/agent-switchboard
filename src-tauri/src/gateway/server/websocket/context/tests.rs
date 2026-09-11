use super::*;

const ROUTE_A: &str = "route-a";

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
    let first = match context
        .prepare(
            ROUTE_A,
            ResponsesRequestMode::Standard,
            &request(first_input, None),
        )
        .unwrap()
    {
        PreparedResponse::Upstream(request) => request,
        PreparedResponse::Prewarm(_) => panic!("must prepare upstream request"),
    };
    let first_body: Value = serde_json::from_slice(&first.body).unwrap();
    assert!(first_body.get("client_metadata").is_none());
    assert_eq!(first_body["reasoning"], json!({ "summary": "auto" }));
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
        .prepare(
            ROUTE_A,
            ResponsesRequestMode::Standard,
            &request(second_input, Some("resp-a")),
        )
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
        .prepare(
            ROUTE_A,
            ResponsesRequestMode::Standard,
            &request(first_input, None),
        )
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
        .prepare(
            ROUTE_A,
            ResponsesRequestMode::Standard,
            &request(json!([]), Some("resp-call")),
        )
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
        .prepare(
            ROUTE_A,
            ResponsesRequestMode::Standard,
            &request(json!([]), Some("missing")),
        )
        .expect_err("missing response must fail");
    assert_eq!(error.code, "previous_response_not_found");
}

#[test]
fn omitted_generate_uses_the_codex_wire_default_of_a_real_response() {
    let mut value: Value = serde_json::from_str(&request(json!([]), None)).unwrap();
    value.as_object_mut().unwrap().remove("generate");
    assert!(matches!(
        ConversationContext::default().prepare(
            ROUTE_A,
            ResponsesRequestMode::Standard,
            &value.to_string()
        ),
        Ok(PreparedResponse::Upstream(_))
    ));
}

#[test]
fn preserves_reasoning_effort_for_the_capability_converter() {
    let mut value: Value = serde_json::from_str(&request(json!([]), None)).unwrap();
    value["reasoning"] = json!({ "effort": "high" });
    let prepared = ConversationContext::default()
        .prepare(ROUTE_A, ResponsesRequestMode::Standard, &value.to_string())
        .expect("reasoning effort belongs to the protocol converter");
    let PreparedResponse::Upstream(request) = prepared else {
        panic!("must prepare upstream request");
    };
    let body: Value = serde_json::from_slice(&request.body).expect("upstream JSON");
    assert_eq!(body["reasoning"], json!({ "effort": "high" }));
}

#[test]
fn replays_tool_search_context_for_dynamic_tool_reconstruction() {
    let mut context = ConversationContext::default();
    let first = match context
        .prepare(
            ROUTE_A,
            ResponsesRequestMode::Standard,
            &request(
                json!([{
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "first" }],
                }]),
                None,
            ),
        )
        .expect("first request")
    {
        PreparedResponse::Upstream(request) => request,
        PreparedResponse::Prewarm(_) => panic!("must prepare upstream request"),
    };
    context
        .record_completed(
            &first,
            &json!({
                "id": "resp-search",
                "object": "response",
                "status": "completed",
                "model": "sandbox-model",
                "output": [{
                    "type": "tool_search_call",
                    "id": "client-generated-search-item",
                    "call_id": "call-search",
                    "status": "completed",
                    "execution": "client",
                    "arguments": { "query": "functions" },
                }],
                "usage": { "input_tokens": 1, "output_tokens": 1, "total_tokens": 2 },
                "error": null,
            }),
        )
        .expect("record search call");
    let next_input = json!([
        {
            "type": "tool_search_output",
            "call_id": "call-search",
            "tools": [{
                "type": "namespace",
                "name": "functions",
                "description": "Workspace functions",
                "tools": [{
                    "type": "function",
                    "name": "list",
                    "description": "List files",
                    "parameters": { "type": "object" },
                }],
            }],
        },
        {
            "type": "message",
            "role": "user",
            "content": [{ "type": "input_text", "text": "continue" }],
        },
    ]);
    let next = match context
        .prepare(
            ROUTE_A,
            ResponsesRequestMode::Standard,
            &request(next_input, Some("resp-search")),
        )
        .expect("tool search replay")
    {
        PreparedResponse::Upstream(request) => request,
        PreparedResponse::Prewarm(_) => panic!("must prepare upstream request"),
    };
    let body: Value = serde_json::from_slice(&next.body).expect("upstream JSON");
    let input = body["input"].as_array().expect("input array");
    assert_eq!(input[1]["type"], "tool_search_call");
    assert!(input[1].get("id").is_none());
    assert_eq!(input[2]["type"], "tool_search_output");
    assert_eq!(input[2]["tools"][0]["tools"][0]["name"], "list");
    assert_eq!(input[3]["content"][0]["text"], "continue");
}

#[test]
fn rejects_invalid_client_generated_tool_search_item_id() {
    for id in [json!(""), json!(42)] {
        let mut context = ConversationContext::default();
        let error = context
            .prepare(
                ROUTE_A,
                ResponsesRequestMode::Standard,
                &request(
                    json!([{
                        "type": "tool_search_call",
                        "id": id,
                        "call_id": "call-search",
                        "status": "completed",
                        "execution": "client",
                        "arguments": { "query": "functions" },
                    }]),
                    None,
                ),
            )
            .expect_err("invalid client-generated item id must be rejected");
        assert_eq!(error.code, "invalid_request");
        assert!(error.message.contains("tool_search_call.id"));
    }
}

#[test]
fn replays_custom_tool_output_for_cross_protocol_conversion() {
    let mut context = ConversationContext::default();
    let first = match context
        .prepare(
            ROUTE_A,
            ResponsesRequestMode::Standard,
            &request(
                json!([{
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "first" }],
                }]),
                None,
            ),
        )
        .expect("first request")
    {
        PreparedResponse::Upstream(request) => request,
        PreparedResponse::Prewarm(_) => panic!("must prepare upstream request"),
    };
    context
        .record_completed(
            &first,
            &json!({
                "id": "resp-custom",
                "object": "response",
                "status": "completed",
                "model": "sandbox-model",
                "output": [{
                    "type": "custom_tool_call",
                    "id": "ctc_1",
                    "call_id": "call-custom",
                    "name": "apply_patch",
                    "input": "*** Begin Patch\n*** End Patch",
                    "status": "completed",
                }],
                "usage": { "input_tokens": 1, "output_tokens": 1, "total_tokens": 2 },
                "error": null,
            }),
        )
        .expect("record custom call");
    let next = match context
        .prepare(
            ROUTE_A,
            ResponsesRequestMode::Standard,
            &request(
                json!([{
                    "type": "custom_tool_call_output",
                    "call_id": "call-custom",
                    "output": "patched",
                }]),
                Some("resp-custom"),
            ),
        )
        .expect("custom output replay")
    {
        PreparedResponse::Upstream(request) => request,
        PreparedResponse::Prewarm(_) => panic!("must prepare upstream request"),
    };
    let body: Value = serde_json::from_slice(&next.body).expect("upstream JSON");
    assert_eq!(body["input"][1]["type"], "custom_tool_call");
    assert_eq!(body["input"][2]["type"], "custom_tool_call_output");
}

#[test]
fn replays_gateway_owned_reasoning_for_an_incremental_request() {
    let mut context = ConversationContext::default();
    let first = match context
        .prepare(
            ROUTE_A,
            ResponsesRequestMode::Standard,
            &request(
                json!([{
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "first" }],
                }]),
                None,
            ),
        )
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
        .prepare(
            ROUTE_A,
            ResponsesRequestMode::Standard,
            &request(json!([]), Some("resp-reasoning")),
        )
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
