use super::*;

const ROUTE_A: &str = "route-a";
const ROUTE_B: &str = "route-b";

fn native(input: Value, previous: Option<&str>) -> String {
    let mut value = json!({"type":"response.create","model":"m","input":input,
        "reasoning":{"effort":"high"},"store":false,"stream":true});
    if let Some(id) = previous {
        value["previous_response_id"] = json!(id);
    }
    value.to_string()
}

fn pending(result: PreparedResponse) -> PendingRequest {
    match result {
        PreparedResponse::Upstream(request) => request,
        _ => panic!("expected upstream"),
    }
}

#[test]
fn native_replay_retains_native_tool_items_and_reasoning_settings() {
    let mut context = ConversationContext::default();
    let first = pending(
        context
            .prepare_native(
                ROUTE_A,
                &native(
                    json!([{"type":"message","role":"user","content":"search"}]),
                    None,
                ),
            )
            .unwrap(),
    );
    let call = json!({"id":"ws_1","type":"web_search_call","status":"completed","action":{"type":"search","query":"rust"},"extra":{"native":true}});
    context
        .record_completed(
            &first,
            &json!({"id":"r1","status":"completed","output":[call],"created_at":9}),
        )
        .unwrap();
    let second = pending(
        context
            .prepare_native(ROUTE_A, &native(json!([]), Some("r1")))
            .unwrap(),
    );
    let body: Value = serde_json::from_slice(&second.body).unwrap();
    assert_eq!(body["input"][1], call);
    assert_eq!(body["reasoning"]["effort"], "high");
    assert!(body.get("previous_response_id").is_none());
}

#[test]
fn native_missing_reference_and_prewarm_are_local() {
    let mut context = ConversationContext::default();
    assert_eq!(
        context
            .prepare_native(ROUTE_A, &native(json!([]), Some("absent")))
            .unwrap_err()
            .code,
        "previous_response_not_found"
    );
    let mut value: Value = serde_json::from_str(&native(json!([]), None)).unwrap();
    value["generate"] = json!(false);
    assert!(matches!(
        context.prepare_native(ROUTE_A, &value.to_string()),
        Ok(PreparedResponse::Prewarm(_))
    ));
}

#[test]
fn compaction_replaces_cached_history_instead_of_replaying_trigger() {
    let mut context = ConversationContext::default();
    let first = pending(context.prepare_native(ROUTE_A, &native(json!([{"type":"message","role":"user","content":"history"},{"type":"compaction_trigger"}]), None)).unwrap());
    let compact = json!({"type":"compaction","encrypted_content":"opaque"});
    context
        .record_completed(
            &first,
            &json!({"id":"c1","status":"completed","output":[compact]}),
        )
        .unwrap();
    let second = pending(
        context
            .prepare_native(ROUTE_A, &native(json!([]), Some("c1")))
            .unwrap(),
    );
    let body: Value = serde_json::from_slice(&second.body).unwrap();
    assert_eq!(body["input"], json!([compact]));
}

#[test]
fn minimal_filters_only_after_previous_input_has_been_reconstructed() {
    let mut context = ConversationContext::default();
    let first = pending(
        context
            .prepare_native(
                ROUTE_A,
                &native(
                    json!([{"type":"message","role":"user","content":"first"}]),
                    None,
                ),
            )
            .unwrap(),
    );
    context
        .record_completed(
            &first,
            &json!({"id":"r1","status":"completed","output":[{
        "type":"message","role":"assistant","content":[{"type":"output_text","text":"answer"}]}]}),
        )
        .unwrap();
    let second = pending(
        context
            .prepare_native(
                ROUTE_A,
                &native(
                    json!([{"type":"message","role":"user","content":"second"}]),
                    Some("r1"),
                ),
            )
            .unwrap(),
    );
    let minimal = crate::gateway::transform::minimal::apply(
        crate::gateway::transform::ConvertedRequest {
            body: second.body,
            stream: true,
        },
        Some(asb_core::contracts::ResponsesOptions {
            request_mode: ResponsesRequestMode::Minimal,
        }),
    )
    .unwrap();
    let body: Value = serde_json::from_slice(&minimal.body).unwrap();
    assert_eq!(body["input"].as_array().unwrap().len(), 3);
    assert!(body.get("previous_response_id").is_none());
    assert!(body.get("reasoning").is_none());
}

fn record_native_output(context: &mut ConversationContext, route: &str, id: &str, output: Value) {
    let first = pending(
        context
            .prepare_native(
                route,
                &native(
                    json!([
                        {"type":"message", "role":"user", "content":"visible history"}
                    ]),
                    None,
                ),
            )
            .unwrap(),
    );
    context
        .record_completed(
            &first,
            &json!({"id":id, "status":"completed", "output":output}),
        )
        .unwrap();
}

fn assert_stale(result: Result<PreparedResponse, ContextError>) {
    let error = result.expect_err("old opaque state must not cross routes");
    assert_eq!(error.code, "previous_response_not_found");
    assert!(error.message.contains("without opaque continuation state"));
}

#[test]
fn native_route_switch_rejects_opaque_history_even_after_prewarm_and_full_retry() {
    let mut context = ConversationContext::for_route(ROUTE_A);
    record_native_output(
        &mut context,
        ROUTE_A,
        "a",
        json!([
            {"type":"reasoning", "encrypted_content":"opaque-a"}
        ]),
    );
    assert_stale(context.prepare_native(ROUTE_B, &native(json!([]), Some("a"))));
    let mut prewarm: Value = serde_json::from_str(&native(json!([]), None)).unwrap();
    prewarm["generate"] = json!(false);
    assert!(matches!(
        context.prepare_native(ROUTE_B, &prewarm.to_string()),
        Ok(PreparedResponse::Prewarm(_))
    ));
    for kind in ["reasoning", "compaction", "context_compaction"] {
        assert_stale(context.prepare_native(
            ROUTE_B,
            &native(
                json!([
                    {"type":kind, "encrypted_content":"opaque-a"}
                ]),
                None,
            ),
        ));
    }
    let visible = json!([
        {"type":"message", "role":"user", "content":"full visible history"},
        {"type":"message", "role":"assistant", "content":"visible answer"},
    ]);
    let retry = pending(
        context
            .prepare_native(ROUTE_B, &native(visible.clone(), None))
            .unwrap(),
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&retry.body).unwrap()["input"],
        visible
    );
    context.record_completed(&retry, &json!({"id":"b", "status":"completed", "output":[
        {"type":"reasoning", "id":"rs_b", "status":"completed", "encrypted_content":"opaque-b"}
    ]})).unwrap();
    assert_stale(context.prepare_native(
        ROUTE_B,
        &native(
            json!([
                {"type":"reasoning", "encrypted_content":"opaque-a"}
            ]),
            None,
        ),
    ));
    let known = json!([{"type":"reasoning", "encrypted_content":"opaque-b"}]);
    let next = pending(
        context
            .prepare_native(ROUTE_B, &native(known.clone(), None))
            .unwrap(),
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&next.body).unwrap()["input"],
        known
    );
    let next = pending(
        context
            .prepare_native(ROUTE_B, &native(json!([]), Some("b")))
            .unwrap(),
    );
    let body = String::from_utf8(next.body).unwrap();
    assert!(body.contains("opaque-b"));
    assert!(!body.contains("opaque-a"));
}

#[test]
fn handshake_binding_protects_a_switch_before_the_first_completion() {
    let mut context = ConversationContext::for_route(ROUTE_A);
    for kind in ["reasoning", "compaction", "context_compaction"] {
        assert_stale(context.prepare_native(
            ROUTE_B,
            &native(
                json!([
                    {"type":kind, "encrypted_content":"unrecorded-route-a-state"}
                ]),
                None,
            ),
        ));
    }
    assert!(matches!(
        context.prepare_native(
            ROUTE_B,
            &native(
                json!([
                    {"type":"message", "role":"user", "content":"visible retry"}
                ]),
                None
            )
        ),
        Ok(PreparedResponse::Upstream(_))
    ));
}

#[test]
fn native_continuation_is_not_restricted_before_the_connection_changes_route() {
    let mut context = ConversationContext::for_route(ROUTE_A);
    let input = json!([{"type":"compaction", "encrypted_content":"same-provider-http-compaction"}]);
    let request = pending(
        context
            .prepare_native(ROUTE_A, &native(input.clone(), None))
            .unwrap(),
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&request.body).unwrap()["input"],
        input
    );
}
