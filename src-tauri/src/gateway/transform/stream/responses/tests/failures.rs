use super::*;

#[test]
fn incomplete_non_token_reasons_and_malformed_details_never_finish_successfully() {
    for details in [
        Value::Null,
        json!({}),
        json!({"reason":"content_filter"}),
        json!({"reason":"upstream_disconnect"}),
        json!({"reason":false}),
        json!("max_output_tokens"),
    ] {
        let mut value = limited(vec![text_item("msg_text", "partial")]);
        value["incomplete_details"] = details;
        let mut harness = Harness::new();
        harness.begin();
        let result = harness.terminal(value);
        assert_rejected(&mut harness, result);
    }
}

#[test]
fn failed_cancelled_and_error_events_preserve_details_and_poison_the_decoder() {
    for kind in ["response.failed", "response.cancelled", "error"] {
        let mut harness = Harness::new();
        harness.begin();
        let detail = json!({"status":"failed","error":{"code":"quota_exceeded","message":"fixture quota exhausted"}});
        let event = if kind == "error" {
            json!({"type":kind,"code":"quota_exceeded","message":"fixture quota exhausted"})
        } else {
            json!({"type":kind,"response":detail})
        };
        let error = harness.push(event).unwrap_err();
        assert!(error.0.contains("quota_exceeded"));
        assert!(error.0.contains("fixture quota exhausted"));
        assert!(harness.terminal(response(vec![])).is_err());
        assert!(harness.finish().is_err());
        assert!(of_type(&harness.events, "message_stop").is_empty());
    }
}

#[test]
fn nominal_terminal_events_cannot_mask_nonterminal_failed_or_conflicting_status() {
    for (event, status) in [
        ("response.completed", "failed"),
        ("response.completed", "cancelled"),
        ("response.completed", "in_progress"),
        ("response.completed", "queued"),
        ("response.completed", "incomplete"),
        ("response.incomplete", "completed"),
    ] {
        let mut value = limited(vec![]);
        value["status"] = json!(status);
        if status == "completed" {
            value["incomplete_details"] = Value::Null;
        }
        let mut harness = Harness::new();
        let result = harness.push(json!({"type":event,"response":value}));
        assert_rejected(&mut harness, result);
    }
}

#[test]
fn embedded_errors_win_over_token_limit_and_completed_status() {
    for status in ["completed", "incomplete"] {
        let mut value = limited(vec![]);
        value["status"] = json!(status);
        value["error"] = json!({"code":"upstream_failure","message":"fixture request failed"});
        let mut harness = Harness::new();
        let error = harness.terminal(value).unwrap_err();
        assert!(error.0.contains("upstream_failure"));
        assert!(error.0.contains("fixture request failed"));
        assert!(harness.events.is_empty());
    }
}

#[test]
fn done_or_eof_cannot_substitute_for_a_terminal_response_even_after_item_done() {
    for marker in [false, true] {
        let item = text_item("msg_text", "answer");
        let mut harness = Harness::new();
        harness.begin();
        harness.push(added(0, item.clone())).unwrap();
        harness.push(done(0, item)).unwrap();
        let result = if marker {
            harness.raw(None, "[DONE]".into())
        } else {
            harness.finish()
        };
        assert_rejected(&mut harness, result);
    }
}

#[test]
fn invalid_or_truncated_final_tool_arguments_cannot_become_a_tool_call() {
    for arguments in [r#"{"path":"#, "", "null", "[]"] {
        let item = tool_item(0, arguments);
        let mut harness = Harness::new();
        harness.begin();
        harness.push(added(0, item.clone())).unwrap();
        harness.push(arguments_delta(0, arguments)).unwrap();
        let result = harness.terminal(limited(vec![item]));
        assert_rejected(&mut harness, result);
    }
    let mut harness = Harness::new();
    harness.begin();
    harness.push(added(0, tool_item(0, "{}"))).unwrap();
    let result = harness.push(json!({"type":"response.function_call_arguments.done",
        "output_index":0,"item_id":"fc_0","arguments":r#"{"path":"#}));
    assert_rejected(&mut harness, result);
}

#[test]
fn data_after_a_terminal_event_or_done_marker_is_a_protocol_error() {
    for marker in [false, true] {
        let mut harness = Harness::new();
        harness.terminal(response(vec![])).unwrap();
        if marker {
            harness.raw(None, "[DONE]".into()).unwrap();
        }
        assert!(harness.push(text_delta(0, "msg_late", "late")).is_err());
        assert!(harness.finish().is_err());
        assert_eq!(of_type(&harness.events, "message_stop").len(), 1);
    }
}

#[test]
fn event_name_and_data_type_must_agree_even_for_terminal_events() {
    let mut harness = Harness::new();
    let data = json!({"type":"response.incomplete","response":limited(vec![])}).to_string();
    let result = harness.raw(Some("response.completed".into()), data);
    assert_rejected(&mut harness, result);
}

#[test]
fn final_output_cannot_omit_items_or_rewrite_an_incomplete_done_snapshot() {
    let mut item = text_item("msg_text", "partial");
    item["status"] = json!("incomplete");
    for changed in [
        limited(vec![]),
        response(vec![text_item("msg_text", "partial")]),
        limited(vec![text_item("msg_text", "different")]),
    ] {
        let mut harness = Harness::new();
        harness.begin();
        harness.push(added(0, item.clone())).unwrap();
        harness.push(done(0, item.clone())).unwrap();
        let result = harness.terminal(changed);
        assert_rejected(&mut harness, result);
    }
}

#[test]
fn incomplete_item_done_disallows_late_payload_mutations() {
    let mut item = text_item("msg_text", "partial");
    item["status"] = json!("incomplete");
    let mut harness = Harness::new();
    harness.begin();
    harness.push(added(0, item.clone())).unwrap();
    harness.push(done(0, item.clone())).unwrap();
    harness.push(done(0, item)).unwrap();
    let result = harness.push(text_delta(0, "msg_text", "late"));
    assert_rejected(&mut harness, result);
}

#[test]
fn progress_cannot_announce_a_failure_or_a_different_response_identity() {
    for response in [
        json!({"id":"resp_lifecycle","model":"test-model","status":"failed"}),
        json!({"id":"resp_other","model":"test-model","status":"in_progress"}),
        json!({"id":"resp_lifecycle","model":"other-model","status":"in_progress"}),
        json!({"id":"resp_lifecycle","model":"test-model","error":{"message":"failed"}}),
    ] {
        let mut harness = Harness::new();
        harness.begin();
        let result = harness.push(json!({"type":"response.in_progress","response":response}));
        assert_rejected(&mut harness, result);
    }
}
