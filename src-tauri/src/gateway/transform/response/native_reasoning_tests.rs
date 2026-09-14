use super::*;
use crate::gateway::transform::{convert_request, SseTranscoder};
use std::io::{Cursor, Read};

fn native_item(encrypted: bool, summary: bool) -> Value {
    let mut item = json!({"id":"rs_native","type":"reasoning","summary":[]});
    if encrypted {
        item["encrypted_content"] = json!("provider-native-opaque");
    }
    if summary {
        item["summary"] = json!([{"type":"summary_text","text":"provider summary"}]);
    }
    item
}

fn response(item: Value) -> Value {
    json!({
        "id":"resp_native", "object":"response", "status":"completed", "model":"gpt-5.4",
        "output":[item, {"type":"function_call","id":"fc_read","call_id":"call_read","name":"Read","arguments":"{\"path\":\"fixture.txt\"}","status":"completed"}],
        "usage":{"input_tokens":8,"output_tokens":3,"total_tokens":11}, "error":null,
        "reasoning":{"effort":"medium","summary":null}, "store":false, "service_tier":"default"
    })
}

fn claude_response(value: &Value, transport: &ReasoningTransport) -> Value {
    serde_json::from_slice(
        &convert_response(
            UpstreamProtocol::Responses,
            UpstreamProtocol::AnthropicMessages,
            &serde_json::to_vec(value).unwrap(),
            Some(transport),
        )
        .unwrap(),
    )
    .unwrap()
}

fn replay(message: &Value, transport: &ReasoningTransport) -> Result<Value, TransformError> {
    let input = json!({"model":"gpt-5.4","max_tokens":128,"messages":[
        {"role":"user","content":"read the fixture"},
        {"role":"assistant","content":message["content"]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"call_read","content":"fixture result"}]}
    ]});
    let converted = convert_request(
        UpstreamProtocol::AnthropicMessages,
        UpstreamProtocol::Responses,
        &serde_json::to_vec(&input).unwrap(),
        None,
        Some(transport),
        None,
    )?;
    serde_json::from_slice(&converted.body).map_err(|e| TransformError(e.to_string()))
}

#[test]
fn claude_native_responses_reasoning_survives_the_full_tool_roundtrip() {
    let transport = ReasoningTransport::from_continuation_key([31; 32]);
    for (encrypted, summary) in [(false, false), (true, false), (false, true), (true, true)] {
        let item = native_item(encrypted, summary);
        let message = claude_response(&response(item.clone()), &transport);
        assert_eq!(message["stop_reason"], "tool_use");
        assert_eq!(message["content"][0]["type"], "redacted_thinking");
        let restored = replay(&message, &transport).unwrap();
        let reason = restored["input"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["type"] == "reasoning")
            .unwrap();
        assert_eq!(reason, &item);
        assert!(!restored.to_string().contains("asb-reasoning-"));
        assert!(restored["input"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["type"] == "function_call_output" && item["call_id"] == "call_read"));
    }
}

#[test]
fn claude_reasoning_replay_rejects_another_backend() {
    let first = ReasoningTransport::from_continuation_key([31; 32]);
    let second = ReasoningTransport::from_continuation_key([32; 32]);
    let message = claude_response(&response(native_item(true, false)), &first);
    assert!(replay(&message, &second).is_err());
}

#[test]
fn claude_native_reasoning_keeps_nullable_optional_fields() {
    let transport = ReasoningTransport::from_continuation_key([31; 32]);
    let mut item = native_item(true, false);
    item["content"] = Value::Null;
    item["status"] = Value::Null;
    let message = claude_response(&response(item.clone()), &transport);
    assert_eq!(replay(&message, &transport).unwrap()["input"][1], item);
    let events = transcode(streamed_response(item), &transport);
    assert_eq!(events.last().unwrap()["type"], "message_stop");
}

#[test]
fn claude_reasoning_preserves_provider_extensions_inside_the_sealed_item() {
    let transport = ReasoningTransport::from_continuation_key([31; 32]);
    let mut item = native_item(true, true);
    item["provider_annotation"] = json!({"retain":"as native data"});
    let message = claude_response(&response(item.clone()), &transport);
    let body = replay(&message, &transport).unwrap();
    assert_eq!(body["input"][1], item);
}

fn event(value: Value) -> String {
    format!(
        "event: {}\ndata: {value}\n\n",
        value["type"].as_str().unwrap()
    )
}

fn streamed_response(item: Value) -> String {
    let final_response = response(item.clone());
    let tool = final_response["output"][1].clone();
    [
        json!({"type":"response.created","response":{"id":"resp_native","model":"gpt-5.4","status":"in_progress","output":[]}}),
        json!({"type":"response.output_item.added","output_index":0,"item":{"type":"reasoning","id":"rs_native","summary":[]}}),
        json!({"type":"response.output_item.done","output_index":0,"item":item}),
        json!({"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","id":"fc_read","call_id":"call_read","name":"Read","arguments":""}}),
        json!({"type":"response.function_call_arguments.delta","output_index":1,"item_id":"fc_read","delta":"{\"path\":\"fixture.txt\"}"}),
        json!({"type":"response.output_item.done","output_index":1,"item":tool}),
        json!({"type":"response.completed","response":final_response}),
    ].into_iter().map(event).collect()
}

fn transcode(source: String, transport: &ReasoningTransport) -> Vec<Value> {
    let mut stream = SseTranscoder::new(
        Cursor::new(source.into_bytes()),
        UpstreamProtocol::Responses,
        UpstreamProtocol::AnthropicMessages,
        1024 * 1024,
        Some(transport),
    )
    .unwrap();
    let mut output = String::new();
    stream.read_to_string(&mut output).unwrap();
    assert!(!stream.failed(), "{output}");
    output
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn claude_stream_native_reasoning_matches_item_done_and_final_snapshot() {
    let transport = ReasoningTransport::from_continuation_key([31; 32]);
    for (encrypted, summary) in [(false, false), (true, false), (false, true), (true, true)] {
        let item = native_item(encrypted, summary);
        let events = transcode(streamed_response(item.clone()), &transport);
        let block = &events
            .iter()
            .find(|event| {
                event["type"] == "content_block_start"
                    && event["content_block"]["type"] == "redacted_thinking"
            })
            .unwrap()["content_block"];
        let reasoning = transport
            .from_continuation(block["data"].as_str().unwrap().into())
            .unwrap();
        assert_eq!(reasoning.responses_input_item().unwrap(), item);
        assert_eq!(
            events
                .iter()
                .filter(|event| event["type"] == "content_block_start")
                .count(),
            2
        );
        assert!(events
            .iter()
            .any(|event| event["type"] == "message_delta"
                && event["delta"]["stop_reason"] == "tool_use"));
        assert_eq!(events.last().unwrap()["type"], "message_stop");
    }
}

#[test]
fn claude_stream_completed_only_preserves_native_reasoning() {
    let transport = ReasoningTransport::from_continuation_key([31; 32]);
    let events = transcode(
        event(json!({"type":"response.completed","response":response(native_item(true, false))})),
        &transport,
    );
    assert_eq!(events.last().unwrap()["type"], "message_stop");
}

#[test]
fn claude_responses_multiple_text_parts_remain_one_output_item() {
    let transport = ReasoningTransport::from_continuation_key([31; 32]);
    let mut value = response(native_item(false, false));
    value["output"][1] = json!({"type":"message","id":"msg_text","role":"assistant","status":"completed","content":[
        {"type":"output_text","text":"one ","annotations":[]},{"type":"output_text","text":"two","annotations":[]}
    ]});
    let message = claude_response(&value, &transport);
    assert_eq!(message["content"][1]["text"], "one two");
    let events = transcode(
        event(json!({"type":"response.completed","response":value})),
        &transport,
    );
    assert_eq!(events.last().unwrap()["type"], "message_stop");
}
