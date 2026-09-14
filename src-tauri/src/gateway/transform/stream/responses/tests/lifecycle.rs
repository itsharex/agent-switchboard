use super::*;

#[test]
fn completed_only_and_regular_completion_match_json() {
    let value = response(vec![text_item("msg_text", "answer")]);
    for started in [false, true] {
        let mut harness = Harness::new();
        if started {
            harness.begin();
        }
        harness.terminal(value.clone()).unwrap();
        harness.finish().unwrap();
        assert_success(&harness, &value);
    }
}

#[test]
fn incomplete_token_limit_finishes_text_and_empty_output_consistently() {
    for output in [vec![], vec![text_item("msg_text", "partial answer")]] {
        for reason in ["max_output_tokens", "max_tokens"] {
            let mut value = limited(output.clone());
            value["incomplete_details"]["reason"] = json!(reason);
            let mut harness = Harness::new();
            harness.terminal(value.clone()).unwrap();
            harness.finish().unwrap();
            assert_success(&harness, &value);
            assert_eq!(message(&harness.events)["stop_reason"], "max_tokens");
        }
    }
}

#[test]
fn incomplete_item_done_waits_for_a_token_limit_terminal_before_closing() {
    let mut item = text_item("msg_partial", "answer so far");
    item["status"] = json!("incomplete");
    let mut harness = Harness::new();
    harness.begin();
    harness.push(added(0, item.clone())).unwrap();
    harness
        .push(text_delta(0, "msg_partial", "answer "))
        .unwrap();
    harness.push(done(0, item.clone())).unwrap();
    assert!(of_type(&harness.events, "content_block_stop").is_empty());
    assert!(of_type(&harness.events, "message_stop").is_empty());
    let value = limited(vec![item]);
    harness.terminal(value.clone()).unwrap();
    harness.finish().unwrap();
    assert_success(&harness, &value);
}

#[test]
fn incomplete_with_valid_tool_arguments_is_max_tokens_not_tool_use() {
    let mut item = tool_item(0, r#"{"path":"fixture"}"#);
    item["status"] = json!("incomplete");
    let mut harness = Harness::new();
    harness.begin();
    harness.push(added(0, item.clone())).unwrap();
    harness.push(arguments_delta(0, r#"{"path":"#)).unwrap();
    harness.push(done(0, item.clone())).unwrap();
    let value = limited(vec![item]);
    harness.terminal(value.clone()).unwrap();
    harness.finish().unwrap();
    assert_success(&harness, &value);
    assert_eq!(message(&harness.events)["stop_reason"], "max_tokens");
}

#[test]
fn terminal_snapshot_can_supply_missing_done_events_without_losing_text() {
    let item = text_item("msg_text", "first second");
    let mut harness = Harness::new();
    harness.begin();
    harness.push(added(0, item.clone())).unwrap();
    harness.push(text_delta(0, "msg_text", "first ")).unwrap();
    let value = response(vec![item]);
    harness.terminal(value.clone()).unwrap();
    harness.finish().unwrap();
    assert_success(&harness, &value);
}

#[test]
fn terminal_event_can_supply_missing_status_and_type_can_supply_event_name() {
    for incomplete in [false, true] {
        let expected = if incomplete {
            limited(vec![])
        } else {
            response(vec![])
        };
        let mut body = expected.clone();
        body.as_object_mut().unwrap().remove("status");
        let event = if incomplete {
            "response.incomplete"
        } else {
            "response.completed"
        };
        let mut harness = Harness::new();
        harness
            .raw(None, json!({"type":event,"response":body}).to_string())
            .unwrap();
        harness.finish().unwrap();
        assert_success(&harness, &expected);
    }
}

#[test]
fn done_marker_after_a_valid_terminal_is_optional_and_not_another_stop() {
    for value in [response(vec![]), limited(vec![])] {
        let mut harness = Harness::new();
        harness.terminal(value.clone()).unwrap();
        harness.raw(None, "[DONE]".into()).unwrap();
        harness.finish().unwrap();
        assert_success(&harness, &value);
        assert!(harness.raw(None, "[DONE]".into()).is_err());
        assert!(harness.finish().is_err());
    }
}

#[test]
fn start_progress_and_terminal_partial_usage_are_cumulative_not_additive() {
    let mut harness = Harness::new();
    harness
        .push(
            json!({"type":"response.created","response":{"id":"resp_lifecycle",
        "model":"test-model","status":"in_progress","usage":{"input_tokens":21,
        "input_tokens_details":{"cached_tokens":9},"output_tokens":0}}}),
        )
        .unwrap();
    harness
        .push(
            json!({"type":"response.in_progress","response":{"id":"resp_lifecycle",
        "model":"test-model","status":"in_progress","usage":{"output_tokens":2}}}),
        )
        .unwrap();
    let mut terminal = limited(vec![text_item("msg_text", "partial")]);
    terminal["usage"] = json!({"output_tokens":5});
    harness.terminal(terminal.clone()).unwrap();
    harness.finish().unwrap();
    terminal["usage"] =
        json!({"input_tokens":21,"output_tokens":5,"input_tokens_details":{"cached_tokens":9}});
    assert_success(&harness, &terminal);
    assert_eq!(
        of_type(&harness.events, "message_start")[0]["message"]["usage"]["input_tokens"],
        12
    );
    assert_eq!(message(&harness.events)["usage"]["output_tokens"], 5);
}

#[test]
fn null_terminal_usage_keeps_observed_counts_while_explicit_zero_overrides_them() {
    for reported in [
        Value::Null,
        json!({"input_tokens":0,"output_tokens":0,"input_tokens_details":{"cached_tokens":0}}),
    ] {
        let mut harness = Harness::new();
        harness
            .push(
                json!({"type":"response.created","response":{"id":"resp_lifecycle",
            "model":"test-model","usage":{"input_tokens":8,"output_tokens":3,
            "input_tokens_details":{"cached_tokens":4}}}}),
            )
            .unwrap();
        let mut terminal = response(vec![]);
        terminal["usage"] = reported.clone();
        harness.terminal(terminal.clone()).unwrap();
        harness.finish().unwrap();
        if reported.is_null() {
            terminal["usage"] = json!({"input_tokens":8,"output_tokens":3,"input_tokens_details":{"cached_tokens":4}});
        }
        assert_success(&harness, &terminal);
    }
}
