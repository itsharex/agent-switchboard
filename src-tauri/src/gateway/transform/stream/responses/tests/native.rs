use super::*;

fn native() -> Value {
    json!({"id":"rs_native","type":"reasoning","status":"completed","summary":[],
        "content":null,"encrypted_content":"provider-native-opaque","provider_extension":{"preserve":true}})
}

#[test]
fn complete_empty_native_reasoning_survives_an_incomplete_terminal_and_tool_interleaving() {
    let item = native();
    let tool = tool_item(1, r#"{"path":"fixture"}"#);
    for reasoning_done in [false, true] {
        let mut harness = Harness::new();
        harness.begin();
        harness.push(added(0, item.clone())).unwrap();
        harness.push(added(1, tool.clone())).unwrap();
        harness
            .push(arguments_delta(1, tool["arguments"].as_str().unwrap()))
            .unwrap();
        harness.push(done(1, tool.clone())).unwrap();
        if reasoning_done {
            harness.push(done(0, item.clone())).unwrap();
        }
        let value = limited(vec![item.clone(), tool.clone()]);
        harness.terminal(value.clone()).unwrap();
        harness.finish().unwrap();
        assert_success(&harness, &value);
        let result = message(&harness.events);
        let continuation = result["content"][0]["data"].as_str().unwrap().to_string();
        let other = ReasoningTransport::from_continuation_key([82; 32]);
        assert!(other.from_continuation(continuation).is_err());
        assert!(!harness
            .events
            .iter()
            .any(|event| event.to_string().contains("provider-native-opaque")));
    }
}

#[test]
fn empty_summary_placeholder_may_be_absent_from_the_final_native_item() {
    let item = native();
    let mut harness = Harness::new();
    harness.begin();
    harness.push(added(0, item.clone())).unwrap();
    harness
        .push(
            json!({"type":"response.reasoning_summary_part.added","output_index":0,
        "item_id":"rs_native","summary_index":0,"part":{"type":"summary_text","text":""}}),
        )
        .unwrap();
    harness.push(done(0, item.clone())).unwrap();
    let value = limited(vec![item]);
    harness.terminal(value.clone()).unwrap();
    harness.finish().unwrap();
    assert_success(&harness, &value);
}

#[test]
fn nonempty_reasoning_cannot_disappear_at_a_token_limit() {
    let item = native();
    let mut harness = Harness::new();
    harness.begin();
    harness.push(added(0, item.clone())).unwrap();
    harness
        .push(
            json!({"type":"response.reasoning_summary_text.delta","output_index":0,
        "item_id":"rs_native","summary_index":0,"delta":"observed reasoning"}),
        )
        .unwrap();
    let result = harness.terminal(limited(vec![item]));
    assert_rejected(&mut harness, result);
}

#[test]
fn completed_reasoning_item_cannot_change_native_ciphertext_or_extensions_at_terminal() {
    let item = native();
    for field in ["encrypted_content", "provider_extension", "id"] {
        let mut harness = Harness::new();
        harness.begin();
        harness.push(added(0, item.clone())).unwrap();
        harness.push(done(0, item.clone())).unwrap();
        let mut changed = item.clone();
        changed[field] = json!("changed");
        let result = harness.terminal(limited(vec![changed]));
        assert_rejected(&mut harness, result);
    }
}

#[test]
fn incomplete_reasoning_or_gateway_ciphertext_never_becomes_a_replayable_native_item() {
    for invalid in [
        json!({"status":"incomplete"}),
        json!({"encrypted_content":"asb-reasoning-v3.forged"}),
    ] {
        let mut item = native();
        for (key, value) in invalid.as_object().unwrap() {
            item[key] = value.clone();
        }
        let mut harness = Harness::new();
        harness.begin();
        let result = harness.terminal(limited(vec![item]));
        assert_rejected(&mut harness, result);
    }
}

#[test]
fn reasoning_item_identity_cannot_disappear_in_a_terminal_snapshot() {
    let mut item = native();
    let mut harness = Harness::new();
    harness.begin();
    harness.push(added(0, item.clone())).unwrap();
    item.as_object_mut().unwrap().remove("id");
    let result = harness.terminal(response(vec![item]));
    assert_rejected(&mut harness, result);
}
