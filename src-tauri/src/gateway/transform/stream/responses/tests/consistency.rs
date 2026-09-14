use super::*;

#[test]
fn out_of_order_interleaved_tools_are_released_in_output_order_and_match_json() {
    let first = tool_item(0, r#"{"first":1}"#);
    let mut second = tool_item(1, r#"{"second":2}"#);
    second["namespace"] = json!("functions.");
    for incomplete in [false, true] {
        let mut harness = Harness::new();
        harness.begin();
        harness.push(added(1, second.clone())).unwrap();
        harness.push(added(0, first.clone())).unwrap();
        harness.push(arguments_delta(1, r#"{"second":"#)).unwrap();
        harness.push(arguments_delta(0, r#"{"first":1}"#)).unwrap();
        harness.push(arguments_delta(1, "2}")).unwrap();
        harness.push(done(1, second.clone())).unwrap();
        assert_eq!(of_type(&harness.events, "content_block_start").len(), 1);
        harness.push(done(0, first.clone())).unwrap();
        let value = if incomplete {
            limited(vec![first.clone(), second.clone()])
        } else {
            response(vec![first.clone(), second.clone()])
        };
        harness.terminal(value.clone()).unwrap();
        harness.finish().unwrap();
        assert_success(&harness, &value);
    }
}

#[test]
fn terminal_snapshot_can_fill_an_earlier_output_gap_before_a_buffered_tool() {
    let tool = tool_item(1, "{}");
    let mut harness = Harness::new();
    harness.begin();
    harness.push(added(1, tool.clone())).unwrap();
    harness.push(done(1, tool.clone())).unwrap();
    assert!(of_type(&harness.events, "content_block_start").is_empty());
    let value = response(vec![text_item("msg_first", "first"), tool]);
    harness.terminal(value.clone()).unwrap();
    harness.finish().unwrap();
    assert_success(&harness, &value);
}

#[test]
fn terminal_text_cannot_extend_or_change_a_closed_block_or_completed_part() {
    for whole_item in [false, true] {
        let original = text_item("msg_text", "first");
        let mut harness = Harness::new();
        harness.begin();
        harness.push(added(0, original.clone())).unwrap();
        harness.push(text_delta(0, "msg_text", "first")).unwrap();
        if whole_item {
            harness.push(done(0, original)).unwrap();
        } else {
            harness
                .push(json!({"type":"response.output_text.done","output_index":0,
                "item_id":"msg_text","content_index":0,"text":"first"}))
                .unwrap();
        }
        let result = harness.terminal(response(vec![text_item("msg_text", "first second")]));
        assert_rejected(&mut harness, result);
    }
}

#[test]
fn terminal_message_ids_and_content_part_types_must_match_the_stream() {
    let original = text_item("msg_text", "answer");
    let mut changed_id = original.clone();
    changed_id["id"] = json!("msg_different");
    let mut changed_kind = original.clone();
    changed_kind["content"] = json!([{"type":"refusal","refusal":"answer"}]);
    for changed in [changed_id, changed_kind] {
        let mut harness = Harness::new();
        harness.begin();
        harness.push(added(0, original.clone())).unwrap();
        harness.push(text_delta(0, "msg_text", "answer")).unwrap();
        let result = harness.terminal(response(vec![changed]));
        assert_rejected(&mut harness, result);
    }
}

#[test]
fn multipart_terminal_text_preserves_part_boundaries_not_just_concatenation() {
    let mut item = text_item("msg_text", "ab");
    item["content"] = json!([{"type":"output_text","text":"a","annotations":[]},
        {"type":"output_text","text":"b","annotations":[]}]);
    let mut harness = Harness::new();
    harness.begin();
    harness.push(added(0, item.clone())).unwrap();
    harness.push(text_delta(0, "msg_text", "ab")).unwrap();
    let result = harness.terminal(response(vec![item]));
    assert_rejected(&mut harness, result);
}

#[test]
fn complete_tool_parameters_can_change_whitespace_but_not_semantics() {
    let initial = tool_item(0, r#"{ "a": 1, "b": 2 }"#);
    let mut harness = Harness::new();
    harness.begin();
    harness.push(added(0, initial.clone())).unwrap();
    harness
        .push(arguments_delta(0, initial["arguments"].as_str().unwrap()))
        .unwrap();
    harness
        .push(
            json!({"type":"response.function_call_arguments.done","output_index":0,
        "item_id":"fc_0","arguments":initial["arguments"]}),
        )
        .unwrap();
    let value = response(vec![tool_item(0, r#"{"b":2,"a":1}"#)]);
    harness.terminal(value.clone()).unwrap();
    harness.finish().unwrap();
    assert_success(&harness, &value);
}

#[test]
fn changed_tool_identity_or_payload_after_argument_done_is_rejected() {
    let original = tool_item(0, r#"{"path":"first"}"#);
    for field in ["id", "call_id", "name", "arguments"] {
        let mut harness = Harness::new();
        harness.begin();
        harness.push(added(0, original.clone())).unwrap();
        harness.push(done(0, original.clone())).unwrap();
        let mut changed = original.clone();
        changed[field] = if field == "arguments" {
            json!(r#"{"path":"other"}"#)
        } else {
            json!("other")
        };
        let result = harness.terminal(response(vec![changed]));
        assert_rejected(&mut harness, result);
    }
}

#[test]
fn incomplete_terminal_cannot_ignore_an_extra_streamed_output_item() {
    let mut harness = Harness::new();
    harness.begin();
    harness
        .push(added(0, text_item("msg_text", "answer")))
        .unwrap();
    let result = harness.terminal(limited(vec![]));
    assert_rejected(&mut harness, result);
}
