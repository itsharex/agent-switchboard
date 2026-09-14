use super::*;

#[test]
fn standard_openai_empty_arguments_first_frame_streams_to_both_clients() {
    for target in TARGETS {
        let mut stream = ChatStream::new(target);
        stream.push(chunk(json!({
            "role": "assistant", "content": null,
            "tool_calls": [{
                "index": 0, "id": "call_weather", "type": "function",
                "function": { "name": "get_weather", "arguments": "" },
            }],
        })));
        let (index, _) = call_start(&stream.events, target, "call_weather");
        assert!(argument_deltas(&stream.events, target, index).is_empty());
        stream.push(chunk(json!({ "tool_calls": [{
            "index": 0, "function": { "arguments": "{\"city\":\"" },
        }] })));
        stream.push(chunk(json!({ "tool_calls": [{
            "index": 0, "function": { "arguments": "Paris\"}" },
        }] })));
        assert_eq!(
            argument_deltas(&stream.events, target, index),
            ["{\"city\":\"", "Paris\"}"]
        );
        stream.push(finish("tool_calls"));
        assert!(events_of(&stream.events, "message_stop").is_empty());
        assert!(events_of(&stream.events, "response.completed").is_empty());
        stream.done();
        assert_function_call(
            &stream.events,
            target,
            "call_weather",
            "get_weather",
            r#"{"city":"Paris"}"#,
        );
        assert_completed(&stream.events, target);
    }
}

#[test]
fn interleaved_parallel_tools_keep_their_arguments_and_output_indices() {
    for target in TARGETS {
        let mut stream = ChatStream::new(target);
        stream.push(chunk(json!({ "content": "Checking." })));
        stream.push(chunk(json!({ "tool_calls": [
            { "index": 0, "id": "call_weather", "type": "function",
                "function": { "name": "get_weather", "arguments": "" } },
            { "index": 1, "id": "call_time", "type": "function",
                "function": { "name": "get_time", "arguments": "" } },
        ] })));
        let (weather_index, _) = call_start(&stream.events, target, "call_weather");
        let (time_index, _) = call_start(&stream.events, target, "call_time");
        assert_eq!((weather_index, time_index), (1, 2));
        stream.push(chunk(json!({ "tool_calls": [
            { "index": 1, "function": { "arguments": "{\"zone\":" } },
            { "index": 0, "function": { "arguments": "{\"city\":" } },
        ] })));
        stream.push(chunk(json!({ "tool_calls": [
            { "index": 0, "function": { "arguments": "\"Oslo\"" } },
            { "index": 1, "function": { "arguments": "\"UTC\"" } },
        ] })));
        let mut end = chunk(json!({ "tool_calls": [
            { "index": 1, "function": { "arguments": "}" } },
            { "index": 0, "function": { "arguments": "}" } },
        ] }));
        end["choices"][0]["finish_reason"] = json!("tool_calls");
        stream.push(end);
        stream.done();
        assert_function_call(
            &stream.events,
            target,
            "call_weather",
            "get_weather",
            r#"{"city":"Oslo"}"#,
        );
        assert_function_call(
            &stream.events,
            target,
            "call_time",
            "get_time",
            r#"{"zone":"UTC"}"#,
        );
        assert_eq!(
            argument_deltas(&stream.events, target, weather_index).len(),
            3
        );
        assert_eq!(argument_deltas(&stream.events, target, time_index).len(), 3);
        assert_completed(&stream.events, target);
    }
}

#[test]
fn arguments_wait_for_id_and_name_in_either_order() {
    for target in TARGETS {
        for name_first in [false, true] {
            let mut stream = ChatStream::new(target);
            stream.push(chunk(json!({ "tool_calls": [
                { "index": 7, "id": null, "type": null,
                    "function": { "name": "", "arguments": "{\"city\":\"" } },
                { "index": 2, "id": "call_ready", "type": "function",
                    "function": { "name": "get_time", "arguments": "{}" } },
            ] })));
            let (ready_index, _) = call_start(&stream.events, target, "call_ready");
            assert_eq!(ready_index, 0);
            let partial_identity = if name_first {
                json!({ "index": 7, "id": "", "function": { "name": "get_weather", "arguments": "\u{5317}" } })
            } else {
                json!({ "index": 7, "id": "call_late", "function": { "name": null, "arguments": "\u{5317}" } })
            };
            let before = stream.events.len();
            stream.push(chunk(json!({ "tool_calls": [partial_identity] })));
            assert_eq!(
                stream.events.len(),
                before,
                "buffer until both identity fields exist"
            );
            stream.push(chunk(json!({ "tool_calls": [{
                "index": 7, "id": "call_late", "type": "function",
                "function": { "name": "get_weather", "arguments": "\u{4eac}\"}" },
            }] })));
            let (late_index, _) = call_start(&stream.events, target, "call_late");
            assert_eq!(late_index, 1);
            let args = "{\"city\":\"\u{5317}\u{4eac}\"}";
            assert_eq!(argument_deltas(&stream.events, target, late_index), [args]);
            stream.push(finish("tool_calls"));
            stream.done();
            assert_function_call(&stream.events, target, "call_ready", "get_time", "{}");
            assert_function_call(&stream.events, target, "call_late", "get_weather", args);
            assert_completed(&stream.events, target);
        }
    }
}

#[test]
fn stable_identity_and_null_or_empty_placeholders_do_not_restart_tools() {
    for target in TARGETS {
        let mut stream = ChatStream::new(target);
        stream.push(chunk(json!({ "tool_calls": [{
            "index": 0, "id": "call_value", "type": "function",
            "function": { "name": "set_value", "arguments": "{\"value\":" },
        }] })));
        let before = stream.events.len();
        for call in [
            json!({ "index": 0, "id": "call_value", "type": "function", "function": { "name": "set_value", "arguments": "" } }),
            json!({ "index": 0, "id": null, "type": null, "function": { "name": null, "arguments": null } }),
            json!({ "index": 0, "id": "", "type": "", "function": null }),
            json!({ "index": 9, "id": null, "function": { "name": "", "arguments": "" } }),
        ] {
            stream.push(chunk(json!({ "tool_calls": [call] })));
        }
        assert_eq!(stream.events.len(), before);
        stream.push(chunk(json!({ "tool_calls": [{
            "index": 0, "id": "", "type": "", "function": { "name": "", "arguments": "42" },
        }] })));
        stream.push(chunk(json!({ "tool_calls": [{
            "index": 0, "id": "call_value", "type": "function",
            "function": { "name": "set_value", "arguments": "}" },
        }] })));
        stream.push(finish("tool_calls"));
        stream.done();
        assert_function_call(
            &stream.events,
            target,
            "call_value",
            "set_value",
            r#"{"value":42}"#,
        );
        assert_completed(&stream.events, target);
    }
}

#[test]
fn empty_and_fragmented_arguments_preserve_codex_custom_tool_events() {
    use crate::gateway::transform::{tool_names::render_target_name, ToolKind};

    let target = UpstreamProtocol::Responses;
    let name = render_target_name(
        UpstreamProtocol::ChatCompletions,
        None,
        "apply_patch",
        ToolKind::Custom,
    )
    .expect("encoded custom tool");
    let mut stream = ChatStream::new(target);
    stream.push(chunk(json!({ "tool_calls": [{
        "index": 0, "id": "call_patch", "type": "function",
        "function": { "name": name, "arguments": "" },
    }] })));
    let (index, start) = call_start(&stream.events, target, "call_patch");
    assert_eq!(start["type"], "custom_tool_call");
    assert_eq!(start["name"], "apply_patch");
    assert!(start.get("namespace").is_none());
    stream.push(chunk(json!({ "tool_calls": [{
        "index": 0, "function": { "arguments": "{\"input\":\"*** Begin Patch\\n" },
    }] })));
    stream.push(chunk(json!({ "tool_calls": [{
        "index": 0, "id": null, "function": { "name": null, "arguments": "*** End Patch\"}" },
    }] })));
    assert!(events_of(&stream.events, "response.custom_tool_call_input.delta").is_empty());
    stream.push(finish("tool_calls"));
    stream.done();
    assert!(argument_deltas(&stream.events, target, index).is_empty());
    let deltas = events_of(&stream.events, "response.custom_tool_call_input.delta");
    assert_eq!(deltas.len(), 1);
    assert_eq!(deltas[0]["delta"], "*** Begin Patch\n*** End Patch");
    let done = events_of(&stream.events, "response.custom_tool_call_input.done");
    assert_eq!(done.len(), 1);
    assert_eq!(done[0]["input"], deltas[0]["delta"]);
    let completed = events_of(&stream.events, "response.completed");
    assert_eq!(
        completed[0]["response"]["output"][0]["input"],
        deltas[0]["delta"]
    );
    assert!(completed[0]["response"]["output"][0]
        .get("namespace")
        .is_none());
    assert_completed(&stream.events, target);
}

#[test]
fn empty_arguments_keep_codex_function_namespaces_reversible() {
    use crate::gateway::transform::{tool_names::render_target_name, ToolKind};

    let target = UpstreamProtocol::Responses;
    let name = render_target_name(
        UpstreamProtocol::ChatCompletions,
        Some("functions."),
        "apply.patch",
        ToolKind::Function,
    )
    .expect("encoded namespace tool");
    let mut stream = ChatStream::new(target);
    stream.push(chunk(json!({ "tool_calls": [{
        "index": 0, "id": "call_patch", "type": "function",
        "function": { "name": name, "arguments": "" },
    }] })));
    let (_, start) = call_start(&stream.events, target, "call_patch");
    assert_eq!(start["namespace"], "functions.");
    stream.push(chunk(json!({ "tool_calls": [{
        "index": 0, "function": { "arguments": "{\"path\":\"a\"}" },
    }] })));
    stream.push(finish("tool_calls"));
    stream.done();
    assert_function_call(
        &stream.events,
        target,
        "call_patch",
        "apply.patch",
        r#"{"path":"a"}"#,
    );
    let completed = events_of(&stream.events, "response.completed");
    assert_eq!(
        completed[0]["response"]["output"][0]["namespace"],
        "functions."
    );
    assert_completed(&stream.events, target);
}
