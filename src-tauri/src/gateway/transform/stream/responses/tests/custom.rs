use super::*;

fn custom(input: &str) -> Value {
    json!({"type":"custom_tool_call","id":"ct_0","call_id":"call_custom",
        "name":"Freeform","input":input,"status":"completed"})
}

#[test]
fn custom_tool_fragments_close_json_only_at_input_completion() {
    for initial in ["", "initial "] {
        let mut harness = Harness::new();
        harness.begin();
        harness.push(added(0, custom(initial))).unwrap();
        for delta in ["first ", "second ", "final"] {
            harness
                .push(
                    json!({"type":"response.custom_tool_call_input.delta","output_index":0,
                "item_id":"ct_0","call_id":"call_custom","delta":delta}),
                )
                .unwrap();
        }
        let final_input = format!("{initial}first second final");
        harness
            .push(
                json!({"type":"response.custom_tool_call_input.done","output_index":0,
            "item_id":"ct_0","call_id":"call_custom","input":final_input}),
            )
            .unwrap();
        harness.push(done(0, custom(&final_input))).unwrap();
        let value = response(vec![custom(&final_input)]);
        harness.terminal(value.clone()).unwrap();
        harness.finish().unwrap();
        assert_success(&harness, &value);
    }
}

#[test]
fn custom_token_limit_snapshot_can_finish_an_open_payload_and_empty_input() {
    for input in ["", "partial tool input"] {
        let mut harness = Harness::new();
        harness.begin();
        harness.push(added(0, custom(""))).unwrap();
        if !input.is_empty() {
            harness
                .push(
                    json!({"type":"response.custom_tool_call_input.delta","output_index":0,
                "item_id":"ct_0","delta":"partial "}),
                )
                .unwrap();
        }
        let mut item = custom(input);
        item["status"] = json!("incomplete");
        let value = limited(vec![item]);
        harness.terminal(value.clone()).unwrap();
        harness.finish().unwrap();
        assert_success(&harness, &value);
    }
}

#[test]
fn custom_input_cannot_change_after_input_done_even_when_final_is_longer() {
    for extra_done in [false, true] {
        let mut harness = Harness::new();
        harness.begin();
        harness.push(added(0, custom(""))).unwrap();
        harness
            .push(
                json!({"type":"response.custom_tool_call_input.done","output_index":0,
            "item_id":"ct_0","input":"first"}),
            )
            .unwrap();
        let result = if extra_done {
            harness.push(
                json!({"type":"response.custom_tool_call_input.done","output_index":0,
                "item_id":"ct_0","input":"first second"}),
            )
        } else {
            harness.terminal(response(vec![custom("first second")]))
        };
        assert_rejected(&mut harness, result);
    }
}
