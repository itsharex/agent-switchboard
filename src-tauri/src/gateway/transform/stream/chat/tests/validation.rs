use super::*;

fn assert_failure(source: &str, target: UpstreamProtocol, expected: &str) {
    let (events, failed) = transcode(source, target);
    assert!(failed, "invalid stream must fail");
    let kind = match target {
        asb_core::UpstreamProtocol::GeminiGenerateContent => panic!("Google native has a separate Claude fixture"),
        UpstreamProtocol::AnthropicMessages => "error",
        UpstreamProtocol::Responses => "response.failed",
        UpstreamProtocol::ChatCompletions => unreachable!(),
    };
    let failures = events_of(&events, kind);
    assert_eq!(failures.len(), 1);
    let diagnostic = failures[0].to_string();
    assert!(diagnostic.contains(expected), "{diagnostic}");
    assert!(
        diagnostic.contains("req-chat-fixture"),
        "retain request identity"
    );
    assert!(diagnostic.contains("https://provider.example/chat/completions"));
    assert_eq!(events.last().unwrap()["type"], kind);
    for terminal in ["message_delta", "message_stop", "response.completed"] {
        assert!(
            events_of(&events, terminal).is_empty(),
            "no successful {terminal}"
        );
    }
}

fn initial_call() -> Value {
    chunk(json!({ "tool_calls": [{
        "index": 0, "id": "call_original", "type": "function",
        "function": { "name": "original_tool", "arguments": "{}" },
    }] }))
}

#[test]
fn conflicting_tool_and_message_identity_is_rejected_for_both_clients() {
    let changed_id = chunk(json!({ "tool_calls": [{ "index": 0, "id": "call_other" }] }));
    let changed_name =
        chunk(json!({ "tool_calls": [{ "index": 0, "function": { "name": "other_tool" } }] }));
    let mut changed_message = chunk(json!({}));
    changed_message["id"] = json!("chat_other");
    let mut changed_model = chunk(json!({}));
    changed_model["model"] = json!("other-model");
    for (change, context) in [
        (changed_id, "Chat tool_call.id"),
        (changed_name, "Chat tool_call.function.name"),
        (changed_message, "Chat SSE id"),
        (changed_model, "Chat SSE model"),
    ] {
        let source = encode_frames(&[initial_call(), change, finish("tool_calls")], true);
        for target in TARGETS {
            assert_failure(&source, target, context);
        }
    }
}

#[test]
fn malformed_tool_delta_shapes_are_not_treated_as_placeholders() {
    let invalid_calls = [
        (json!({ "index": 0, "id": 7 }), "id"),
        (json!({ "index": 0, "type": 7 }), "type"),
        (json!({ "index": 0, "type": "custom" }), "function"),
        (json!({ "index": 0, "function": "" }), "function"),
        (json!({ "index": 0, "function": { "name": false } }), "name"),
        (
            json!({ "index": 0, "function": { "arguments": {} } }),
            "arguments",
        ),
        (
            json!({ "index": 0, "function": { "arguments": 0 } }),
            "arguments",
        ),
        (
            json!({ "index": 0, "function": { "extra": "payload" } }),
            "extra",
        ),
        (json!({ "index": -1, "id": "call_bad" }), "index"),
        (json!({ "index": "0", "id": "call_bad" }), "index"),
        (json!({ "index": null, "id": "call_bad" }), "index"),
        (json!({ "id": "call_bad" }), "index"),
    ];
    for (call, field) in invalid_calls {
        let source = encode_frames(
            &[
                chunk(json!({ "role": "assistant" })),
                chunk(json!({ "tool_calls": [call] })),
                finish("tool_calls"),
            ],
            true,
        );
        for target in TARGETS {
            assert_failure(&source, target, field);
        }
    }
}

#[test]
fn malformed_delta_fields_remain_errors() {
    for (delta, field) in [
        (json!({ "role": 1 }), "role"),
        (json!({ "content": {} }), "content"),
        (json!({ "reasoning_content": [] }), "reasoning_content"),
        (json!({ "tool_calls": {} }), "tool_calls"),
    ] {
        let source = encode_frames(&[chunk(delta), finish("stop")], true);
        for target in TARGETS {
            assert_failure(&source, target, field);
        }
    }
}

#[test]
fn invalid_or_incomplete_arguments_cannot_produce_success() {
    for arguments in ["", "{", "{\"value\":", "{\"value\":1,}", "{} trailing"] {
        let mut call = initial_call();
        call["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"] = json!(arguments);
        let source = encode_frames(&[call, finish("tool_calls")], true);
        for target in TARGETS {
            assert_failure(&source, target, "JSON");
        }
    }
}

#[test]
fn unresolved_tool_identity_is_not_fabricated_at_completion() {
    for call in [
        json!({ "index": 0, "function": { "arguments": "{}" } }),
        json!({ "index": 0, "id": "call_missing_name", "function": { "name": "", "arguments": "{}" } }),
        json!({ "index": 0, "id": null, "function": { "name": "known_tool", "arguments": "{}" } }),
    ] {
        let source = encode_frames(
            &[chunk(json!({ "tool_calls": [call] })), finish("tool_calls")],
            true,
        );
        for target in TARGETS {
            assert_failure(&source, target, "Chat SSE 工具调用缺少 id 或名称");
        }
    }
}

#[test]
fn malformed_json_and_truncated_streams_keep_parse_diagnostics() {
    let prefix = chunk(json!({ "content": "partial" }));
    let incomplete_frame = format!(
        "{}data: {{",
        encode_frames(std::slice::from_ref(&prefix), false)
    );
    let malformed_frame = format!(
        "{}data: {{invalid}}\n\n",
        encode_frames(std::slice::from_ref(&prefix), false)
    );
    let unfinished = encode_frames(std::slice::from_ref(&prefix), false);
    let missing_done = encode_frames(&[prefix.clone(), finish("stop")], false);
    let missing_reason = encode_frames(&[prefix], true);
    for (source, expected) in [
        (incomplete_frame, "上游 SSE 在完整帧前结束"),
        (malformed_frame, "Chat SSE data 不是有效 JSON"),
        (unfinished, "[DONE]"),
        (missing_done, "[DONE]"),
        (missing_reason, "finish_reason"),
    ] {
        for target in TARGETS {
            assert_failure(&source, target, expected);
        }
    }
}

#[test]
fn conflicting_finish_reasons_are_not_silently_overwritten() {
    let source = encode_frames(
        &[initial_call(), finish("tool_calls"), finish("stop")],
        true,
    );
    for target in TARGETS {
        assert_failure(&source, target, "finish_reason");
    }
}

#[test]
fn done_remains_terminal_for_both_converters() {
    for target in TARGETS {
        for data in [
            "[DONE]".to_string(),
            chunk(json!({ "content": "late" })).to_string(),
        ] {
            let mut stream = ChatStream::new(target);
            stream.push(chunk(json!({ "content": "finished" })));
            stream.push(finish("stop"));
            stream.done();
            let error = stream
                .transformer
                .on_frame(Frame { event: None, data })
                .expect_err("no data after terminal frame");
            assert!(error.to_string().contains("[DONE]"));
        }
    }
}

#[test]
fn real_reasoning_without_a_continuation_transport_is_not_faked() {
    let source = encode_frames(
        &[
            chunk(json!({ "reasoning_content": "private reasoning" })),
            chunk(json!({ "content": "answer" })),
            finish("stop"),
        ],
        true,
    );
    for target in TARGETS {
        assert_failure(&source, target, "当前转换缺少本机推理续接通道");
    }
}
