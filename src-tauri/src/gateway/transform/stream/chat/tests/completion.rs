use super::*;

#[test]
fn final_arguments_and_usage_after_repeated_finish_are_not_lost() {
    for target in TARGETS {
        let mut stream = ChatStream::new(target);
        stream.push(chunk(json!({ "tool_calls": [{
            "index": 0, "id": "call_value", "type": "function",
            "function": { "name": "set_value", "arguments": "{" },
        }] })));
        let mut end = chunk(json!({ "tool_calls": [{
            "index": 0, "function": { "arguments": "\"value\":1}" },
        }] }));
        end["choices"][0]["finish_reason"] = json!("tool_calls");
        stream.push(end);
        let mut repeated_end = finish("tool_calls");
        repeated_end["choices"][0]["delta"] = Value::Null;
        repeated_end["usage"] =
            json!({ "prompt_tokens": 40, "completion_tokens": 8, "total_tokens": 48 });
        stream.push(repeated_end);
        stream.push(json!({
            "id": null, "model": "", "choices": [],
            "usage": {
                "prompt_tokens": 41, "completion_tokens": 9, "total_tokens": 50,
                "prompt_tokens_details": { "cached_tokens": 5 },
                "completion_tokens_details": { "reasoning_tokens": 3 },
            },
        }));
        stream.push(json!({ "choices": [], "usage": null }));
        for kind in [
            "content_block_stop",
            "message_delta",
            "message_stop",
            "response.completed",
        ] {
            assert!(
                events_of(&stream.events, kind).is_empty(),
                "no premature {kind}"
            );
        }
        stream.done();
        assert_function_call(
            &stream.events,
            target,
            "call_value",
            "set_value",
            r#"{"value":1}"#,
        );
        match target {
            asb_core::UpstreamProtocol::GeminiGenerateContent => {
                panic!("Google native has a separate Claude fixture")
            }
            UpstreamProtocol::AnthropicMessages => {
                let deltas = events_of(&stream.events, "message_delta");
                assert_eq!(deltas.len(), 1);
                assert_eq!(deltas[0]["delta"]["stop_reason"], "tool_use");
                assert_eq!(
                    deltas[0]["usage"],
                    json!({
                        "input_tokens": 36, "output_tokens": 9, "cache_read_input_tokens": 5,
                    })
                );
            }
            UpstreamProtocol::Responses => {
                let completed = events_of(&stream.events, "response.completed");
                assert_eq!(
                    completed[0]["response"]["usage"],
                    json!({
                        "input_tokens": 41, "output_tokens": 9, "total_tokens": 50,
                        "input_tokens_details": { "cached_tokens": 5 },
                        "output_tokens_details": { "reasoning_tokens": 3 },
                    })
                );
            }
            UpstreamProtocol::ChatCompletions => unreachable!(),
        }
        assert_completed(&stream.events, target);
    }
}

#[test]
fn empty_reasoning_and_placeholder_fields_leave_a_single_text_block() {
    for target in TARGETS {
        let mut stream = ChatStream::new(target);
        stream.push(chunk(
            json!({ "role": "assistant", "content": null, "tool_calls": null }),
        ));
        for text in ["A complete", " answer"] {
            stream.push(chunk(json!({
                "content": text, "reasoning_content": "", "role": null,
                "refusal": null, "tool_calls": [],
            })));
        }
        let before = stream.events.len();
        stream.push(chunk(json!({
            "content": "", "refusal": "", "reasoning_content": null,
            "tool_calls": [{ "index": 0, "id": null, "type": "", "function": null }],
        })));
        assert_eq!(stream.events.len(), before);
        stream.push(finish("stop"));
        stream.done();
        match target {
            asb_core::UpstreamProtocol::GeminiGenerateContent => {
                panic!("Google native has a separate Claude fixture")
            }
            UpstreamProtocol::AnthropicMessages => {
                let starts = events_of(&stream.events, "content_block_start");
                assert_eq!(starts.len(), 1);
                assert_eq!(starts[0]["index"], 0);
                assert_eq!(starts[0]["content_block"]["type"], "text");
                let deltas = events_of(&stream.events, "content_block_delta");
                assert!(deltas
                    .iter()
                    .all(|event| event["index"] == 0 && event["delta"]["type"] == "text_delta"));
                assert_eq!(
                    deltas
                        .iter()
                        .map(|event| event["delta"]["text"].as_str().unwrap())
                        .collect::<String>(),
                    "A complete answer"
                );
                let terminal = events_of(&stream.events, "message_delta");
                assert_eq!(terminal[0]["delta"]["stop_reason"], "end_turn");
                assert_eq!(terminal[0]["usage"], json!({ "output_tokens": 0 }));
            }
            UpstreamProtocol::Responses => {
                let starts = events_of(&stream.events, "response.output_item.added");
                assert_eq!(starts.len(), 1);
                assert_eq!(starts[0]["output_index"], 0);
                assert_eq!(starts[0]["item"]["type"], "message");
                let deltas = events_of(&stream.events, "response.output_text.delta");
                assert!(deltas.iter().all(|event| event["output_index"] == 0));
                assert_eq!(
                    deltas
                        .iter()
                        .map(|event| event["delta"].as_str().unwrap())
                        .collect::<String>(),
                    "A complete answer"
                );
                let completed = events_of(&stream.events, "response.completed");
                assert_eq!(
                    completed[0]["response"]["output"].as_array().unwrap().len(),
                    1
                );
            }
            UpstreamProtocol::ChatCompletions => unreachable!(),
        }
        assert_completed(&stream.events, target);
    }
}

#[test]
fn bytewise_sse_and_utf8_boundaries_do_not_drop_tool_arguments() {
    let arguments = "{\"city\":\"\u{5317}\u{4eac}\"}";
    let source = encode_frames(
        &[
            chunk(json!({ "tool_calls": [{
            "index": 0, "id": "call_city", "type": "function",
            "function": { "name": "get_weather", "arguments": "" },
        }] })),
            chunk(
                json!({ "tool_calls": [{ "index": 0, "function": { "arguments": arguments } }] }),
            ),
            finish("tool_calls"),
            json!({ "choices": [], "usage": { "prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6 } }),
        ],
        true,
    );
    for target in TARGETS {
        let (events, failed) = transcode(&source, target);
        assert!(!failed);
        assert_function_call(&events, target, "call_city", "get_weather", arguments);
        assert_completed(&events, target);
    }
}
