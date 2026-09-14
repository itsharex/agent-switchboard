use super::*;

fn single_result(content: Option<Value>, is_error: Option<Value>) -> Value {
    let mut result = json!({ "type": "tool_result", "tool_use_id": "capture-1" });
    if let Some(content) = content {
        result["content"] = content;
    }
    if let Some(is_error) = is_error {
        result["is_error"] = is_error;
    }
    let mut input = request("gpt-5.4");
    input["messages"].as_array_mut().unwrap().extend([
        json!({ "role": "assistant", "content": [tool_call("capture-1")] }),
        json!({ "role": "user", "content": [result] }),
    ]);
    input
}

fn rendered_result(output: &Value, target: UpstreamProtocol) -> &Value {
    let (items, field, expected) = match target {
        UpstreamProtocol::ChatCompletions => (&output["messages"], "role", "tool"),
        UpstreamProtocol::Responses => (&output["input"], "type", "function_call_output"),
        _ => unreachable!(),
    };
    items
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item[field] == expected)
        .unwrap()
}

#[test]
fn tool_error_flags_survive_empty_text_and_image_results_for_both_targets() {
    for content in [
        None,
        Some(json!("")),
        Some(json!("failed to capture")),
        Some(json!([])),
        Some(json!([{ "type": "text", "text": "partial" }, image("ERROR_IMAGE")])),
    ] {
        for flag in [None, Some(json!(false)), Some(json!(true))] {
            let is_error = flag == Some(json!(true));
            let input = single_result(content.clone(), flag);
            for target in TARGETS {
                let output = convert(&input, target).expect("tool errors remain usable history");
                let result = rendered_result(&output, target);
                let text = serde_json::to_string(result).unwrap();
                assert_eq!(text.contains(TOOL_RESULT_ERROR_MARKER), is_error);
                match target {
                    UpstreamProtocol::ChatCompletions => {
                        assert_eq!(result["tool_call_id"], "capture-1");
                        assert!(result["content"].is_string());
                        assert!(!text.contains("data:image"));
                    }
                    UpstreamProtocol::Responses => {
                        assert_eq!(result["call_id"], "capture-1");
                        if is_error {
                            assert_eq!(result["output"][0]["text"], TOOL_RESULT_ERROR_MARKER);
                        }
                    }
                    _ => unreachable!(),
                }
                if content.as_ref().is_some_and(Value::is_array)
                    && content.as_ref().unwrap().as_array().unwrap().len() == 2
                {
                    assert!(serde_json::to_string(&output)
                        .unwrap()
                        .contains("data:image/png;base64,ERROR_IMAGE"));
                }
            }
        }
    }
}

#[test]
fn malformed_tool_error_flags_fail_instead_of_becoming_success() {
    for flag in [json!("true"), json!(1), Value::Null, json!({})] {
        let input = single_result(Some(json!("failed")), Some(flag));
        for target in TARGETS {
            assert!(convert(&input, target).unwrap_err().0.contains("is_error"));
        }
    }
}

fn parallel_request(split_turn: bool) -> Value {
    let first = json!({
        "type": "tool_result", "tool_use_id": "first", "content": [
            { "type": "text", "text": "before image" }, image("FIRST_IMAGE"),
            { "type": "text", "text": "after image" },
        ],
    });
    let second = json!({
        "type": "tool_result", "tool_use_id": "second", "is_error": true,
        "content": [image("SECOND_IMAGE"), { "type": "text", "text": "capture failed" }],
    });
    let mut input = request("gpt-5.4");
    let messages = input["messages"].as_array_mut().unwrap();
    messages
        .push(json!({ "role": "assistant", "content": [tool_call("first"), tool_call("second")] }));
    if split_turn {
        messages.extend([
            json!({ "role": "user", "content": [first, { "type": "text", "text": "between" }] }),
            json!({ "role": "user", "content": [second, { "type": "text", "text": "after" }] }),
        ]);
    } else {
        messages.push(json!({ "role": "user", "content": [
            first, { "type": "text", "text": "between" }, second, { "type": "text", "text": "after" },
        ] }));
    }
    messages.extend([
        json!({ "role": "assistant", "content": [tool_call("retry")] }),
        json!({ "role": "user", "content": [{ "type": "tool_result", "tool_use_id": "retry", "content": "recovered" }] }),
    ]);
    input
}

#[test]
fn chat_parallel_results_are_adjacent_and_media_precedes_ordinary_user_content() {
    for split in [false, true] {
        let output = convert(&parallel_request(split), UpstreamProtocol::ChatCompletions).unwrap();
        let messages = output["messages"].as_array().unwrap();
        assert_eq!(messages[1]["tool_calls"].as_array().unwrap().len(), 2);
        assert_eq!(messages[2]["tool_call_id"], "first");
        assert_eq!(messages[3]["tool_call_id"], "second");
        for index in [2, 3] {
            assert_eq!(messages[index]["role"], "tool");
            assert!(messages[index]["content"]
                .as_str()
                .unwrap()
                .contains(TOOL_RESULT_MEDIA_MARKER));
            assert!(!messages[index]["content"]
                .as_str()
                .unwrap()
                .contains("data:image"));
        }
        assert!(messages[3]["content"]
            .as_str()
            .unwrap()
            .starts_with(TOOL_RESULT_ERROR_MARKER));
        assert_eq!(messages[4]["role"], "user");
        assert_eq!(
            messages[4]["content"][0]["text"],
            "[asb: media output of tool call first]"
        );
        assert_eq!(
            messages[4]["content"][1]["image_url"]["url"],
            "data:image/png;base64,FIRST_IMAGE"
        );
        assert_eq!(
            messages[4]["content"][2]["text"],
            "[asb: media output of tool call second]"
        );
        assert_eq!(
            messages[4]["content"][3]["image_url"]["url"],
            "data:image/png;base64,SECOND_IMAGE"
        );
        let ordinary: String = messages[5..messages.len() - 2]
            .iter()
            .map(|message| message["content"].as_str().unwrap())
            .collect();
        assert_eq!(ordinary, "betweenafter");
        assert_eq!(messages[messages.len() - 2]["tool_calls"][0]["id"], "retry");
        assert_eq!(messages.last().unwrap()["content"], "recovered");
        assert_eq!(messages.last().unwrap()["tool_call_id"], "retry");
    }
}

#[test]
fn responses_parallel_error_results_retain_native_images_and_continue_the_loop() {
    for split in [false, true] {
        let output = convert(&parallel_request(split), UpstreamProtocol::Responses).unwrap();
        let results: Vec<_> = output["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["type"] == "function_call_output")
            .collect();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["call_id"], "first");
        assert_eq!(results[1]["call_id"], "second");
        assert_eq!(results[2]["call_id"], "retry");
        assert_eq!(results[0]["output"][0]["text"], "before image");
        assert_eq!(
            results[0]["output"][1]["image_url"],
            "data:image/png;base64,FIRST_IMAGE"
        );
        assert_eq!(results[0]["output"][2]["text"], "after image");
        assert_eq!(results[1]["output"][0]["text"], TOOL_RESULT_ERROR_MARKER);
        assert_eq!(
            results[1]["output"][1]["image_url"],
            "data:image/png;base64,SECOND_IMAGE"
        );
        assert_eq!(results[1]["output"][2]["text"], "capture failed");
        assert_eq!(results[2]["output"][0]["text"], "recovered");
    }
}

#[test]
fn chat_media_waits_for_results_across_a_text_only_user_message() {
    let mut input = parallel_request(true);
    input["messages"].as_array_mut().unwrap().insert(
        3,
        json!({
            "role": "user", "content": "interleaved note",
        }),
    );
    let output = convert(&input, UpstreamProtocol::ChatCompletions).unwrap();
    assert_eq!(output["messages"][2]["tool_call_id"], "first");
    assert_eq!(output["messages"][3]["tool_call_id"], "second");
    assert_eq!(
        output["messages"][4]["content"][1]["image_url"]["url"],
        "data:image/png;base64,FIRST_IMAGE"
    );
    assert_eq!(output["messages"][5]["content"], "between");
    assert_eq!(output["messages"][6]["content"], "interleaved note");
    assert_eq!(output["messages"][7]["content"], "after");
}

#[test]
fn out_of_order_parallel_completions_keep_their_media_and_error_associations() {
    let mut input = parallel_request(false);
    input["messages"][2]["content"]
        .as_array_mut()
        .unwrap()
        .swap(0, 2);
    let output = convert(&input, UpstreamProtocol::ChatCompletions).unwrap();
    assert_eq!(output["messages"][2]["tool_call_id"], "second");
    assert!(output["messages"][2]["content"]
        .as_str()
        .unwrap()
        .starts_with(TOOL_RESULT_ERROR_MARKER));
    assert_eq!(output["messages"][3]["tool_call_id"], "first");
    assert_eq!(
        output["messages"][4]["content"][0]["text"],
        "[asb: media output of tool call second]"
    );
    assert_eq!(
        output["messages"][4]["content"][1]["image_url"]["url"],
        "data:image/png;base64,SECOND_IMAGE"
    );
    assert_eq!(
        output["messages"][4]["content"][2]["text"],
        "[asb: media output of tool call first]"
    );
}
