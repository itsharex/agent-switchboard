use super::*;
use serde_json::json;
use std::collections::VecDeque;
use std::io::Cursor;

struct StagedReader {
    chunks: VecDeque<Vec<u8>>,
}

impl StagedReader {
    fn new(chunks: impl IntoIterator<Item = &'static [u8]>) -> Self {
        Self {
            chunks: chunks.into_iter().map(ToOwned::to_owned).collect(),
        }
    }
}

impl Read for StagedReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let Some(chunk) = self.chunks.pop_front() else {
            return Ok(0);
        };
        assert!(chunk.len() <= output.len());
        output[..chunk.len()].copy_from_slice(&chunk);
        Ok(chunk.len())
    }
}

#[test]
fn emits_the_first_target_event_before_upstream_eof() {
    let mut stream = SseTranscoder::new(
            StagedReader::new([
                b"data: {\"id\":\"chat_1\",\"model\":\"m\",\"service_tier\":\"on_demand\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null},\"finish_reason\":null}]}\n\n" as &[u8],
                b"data: {\"id\":\"chat_1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n" as &[u8],
            ]),
            UpstreamProtocol::ChatCompletions,
            UpstreamProtocol::Responses,
            1024 * 1024,
            None,
        )
        .expect("supported route");
    let mut first = [0_u8; 4096];
    let count = stream.read(&mut first).expect("first target frame");
    let first = std::str::from_utf8(&first[..count]).expect("utf8");
    assert!(first.contains("response.created"));
    assert!(!first.contains("hello"));

    let mut rest = String::new();
    stream
        .read_to_string(&mut rest)
        .expect("remaining target frames");
    assert!(rest.contains("response.output_text.delta"));
    assert!(rest.contains("response.completed"));
}

#[test]
fn chat_text_stream_converts_to_anthropic() {
    let source = format!(
        "data: {}\n\ndata: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({
            "id": "chat_1",
            "model": "sandbox",
            "choices": [{
                "index": 0,
                "delta": { "role": "assistant" },
                "finish_reason": null
            }]
        }),
        json!({
            "id": "chat_1",
            "model": "sandbox",
            "choices": [{
                "index": 0,
                "delta": { "content": "hello" },
                "finish_reason": null
            }]
        }),
        json!({
            "id": "chat_1",
            "model": "sandbox",
            "choices": [{ "index": 0, "delta": {}, "finish_reason": "stop" }]
        }),
    );
    let mut stream = SseTranscoder::new(
        Cursor::new(source.into_bytes()),
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::AnthropicMessages,
        1024 * 1024,
        None,
    )
    .expect("supported route");
    let mut output = String::new();
    stream
        .read_to_string(&mut output)
        .expect("converted stream");

    assert!(output.contains("event: message_start"));
    assert!(output.contains("event: content_block_delta"));
    assert!(output.contains("hello"));
    assert!(output.contains("event: message_stop"));
}

#[test]
fn chat_tool_stream_restores_a_responses_namespace() {
    let name = crate::gateway::transform::tool_names::render_target_name(
        UpstreamProtocol::ChatCompletions,
        Some("functions."),
        "apply.patch",
        crate::gateway::transform::ToolKind::Function,
    )
    .expect("encode namespace tool");
    let source = format!(
        "data: {}\n\ndata: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({
            "id": "chat_1",
            "model": "sandbox",
            "choices": [{
                "index": 0,
                "delta": { "role": "assistant" },
                "finish_reason": null
            }]
        }),
        json!({
            "id": "chat_1",
            "model": "sandbox",
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "type": "function",
                        "function": { "name": name, "arguments": "{\"path\":\"a\"}" }
                    }]
                },
                "finish_reason": null
            }]
        }),
        json!({
            "id": "chat_1",
            "model": "sandbox",
            "choices": [{ "index": 0, "delta": {}, "finish_reason": "tool_calls" }]
        }),
    );
    let mut stream = SseTranscoder::new(
        Cursor::new(source.into_bytes()),
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::Responses,
        1024 * 1024,
        None,
    )
    .expect("supported route");
    let mut output = String::new();
    stream
        .read_to_string(&mut output)
        .expect("converted stream");

    assert!(output.contains("\"name\":\"apply.patch\""));
    assert!(output.contains("\"namespace\":\"functions.\""));
    assert!(!output.contains("asbns_n_"));
}

#[test]
fn chat_custom_tool_stream_restores_codex_custom_events() {
    let name = crate::gateway::transform::tool_names::render_target_name(
        UpstreamProtocol::ChatCompletions,
        None,
        "apply_patch",
        crate::gateway::transform::ToolKind::Custom,
    )
    .expect("encode custom tool");
    let arguments = json!({ "input": "*** Begin Patch\n*** End Patch" }).to_string();
    let source = format!(
        "data: {}\n\ndata: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({"id":"chat_1","model":"sandbox","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}),
        json!({"id":"chat_1","model":"sandbox","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":name,"arguments":arguments}}]},"finish_reason":null}]}),
        json!({"id":"chat_1","model":"sandbox","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
    );
    let mut stream = SseTranscoder::new(
        Cursor::new(source.into_bytes()),
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::Responses,
        1024 * 1024,
        None,
    )
    .expect("supported route");
    let mut output = String::new();
    stream
        .read_to_string(&mut output)
        .expect("converted stream");
    assert!(output.contains("response.custom_tool_call_input.delta"));
    assert!(output.contains("response.custom_tool_call_input.done"));
    assert!(output.contains("\"type\":\"custom_tool_call\""));
    assert!(output.contains("\"input\":\"*** Begin Patch\\n*** End Patch\""));
    assert!(!output.contains("response.function_call_arguments.delta"));
}

#[test]
fn responses_tool_stream_encodes_a_namespace_for_anthropic() {
    let response = json!({
        "id": "resp_1",
        "object": "response",
        "status": "completed",
        "model": "sandbox",
        "output": [{
            "type": "function_call",
            "id": "fc_1",
            "call_id": "call_1",
            "name": "apply.patch",
            "namespace": "functions.",
            "arguments": "{\"path\":\"a\"}",
            "status": "completed"
        }],
        "usage": { "input_tokens": 1, "output_tokens": 1, "total_tokens": 2 },
        "error": null
    });
    let source = format!(
        "event: response.created\ndata: {}\n\nevent: response.completed\ndata: {}\n\n",
        json!({
            "type": "response.created",
            "response": {
                "id": "resp_1",
                "object": "response",
                "status": "in_progress",
                "model": "sandbox",
                "output": []
            }
        }),
        json!({ "type": "response.completed", "response": response }),
    );
    let mut stream = SseTranscoder::new(
        Cursor::new(source.into_bytes()),
        UpstreamProtocol::Responses,
        UpstreamProtocol::AnthropicMessages,
        1024 * 1024,
        None,
    )
    .expect("supported route");
    let mut output = String::new();
    stream
        .read_to_string(&mut output)
        .expect("converted stream");

    assert!(output.contains("\"type\":\"tool_use\""));
    assert!(output.contains("\"name\":\"asbns_n_"));
    assert!(!output.contains("\"namespace\""));
}

#[test]
fn responses_reasoning_summary_stream_is_opaque_and_ordered() {
    let response = json!({
        "id": "resp_reasoning",
        "object": "response",
        "status": "completed",
        "model": "sandbox",
        "output": [
            {
                "id": "rs_1",
                "type": "reasoning",
                "status": "completed",
                "summary": [{ "type": "summary_text", "text": "private plan" }]
            },
            {
                "id": "msg_1",
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "visible", "annotations": [] }]
            }
        ],
        "usage": { "input_tokens": 1, "output_tokens": 2, "total_tokens": 3 },
        "error": null
    });
    let source = format!(
        "event: response.created\ndata: {}\n\nevent: response.output_item.added\ndata: {}\n\nevent: response.reasoning_summary_part.added\ndata: {}\n\nevent: response.reasoning_summary_text.delta\ndata: {}\n\nevent: response.reasoning_summary_text.done\ndata: {}\n\nevent: response.output_item.done\ndata: {}\n\nevent: response.output_item.added\ndata: {}\n\nevent: response.content_part.added\ndata: {}\n\nevent: response.output_text.delta\ndata: {}\n\nevent: response.output_text.done\ndata: {}\n\nevent: response.content_part.done\ndata: {}\n\nevent: response.output_item.done\ndata: {}\n\nevent: response.completed\ndata: {}\n\n",
        json!({
            "type": "response.created",
            "response": { "id": "resp_reasoning", "object": "response", "status": "in_progress", "model": "sandbox", "output": [] }
        }),
        json!({ "type": "response.output_item.added", "output_index": 0, "item": { "id": "rs_1", "type": "reasoning", "status": "in_progress", "summary": [] } }),
        json!({ "type": "response.reasoning_summary_part.added", "item_id": "rs_1", "output_index": 0, "summary_index": 0, "part": { "type": "summary_text", "text": "" }, "sequence_number": 1 }),
        json!({ "type": "response.reasoning_summary_text.delta", "item_id": "rs_1", "output_index": 0, "summary_index": 0, "delta": "private plan", "sequence_number": 2 }),
        json!({ "type": "response.reasoning_summary_text.done", "item_id": "rs_1", "output_index": 0, "summary_index": 0, "text": "private plan", "sequence_number": 3 }),
        json!({ "type": "response.output_item.done", "output_index": 0, "item": response["output"][0], "sequence_number": 4 }),
        json!({ "type": "response.output_item.added", "output_index": 1, "item": { "id": "msg_1", "type": "message", "status": "in_progress", "role": "assistant", "content": [] } }),
        json!({ "type": "response.content_part.added", "item_id": "msg_1", "output_index": 1, "content_index": 0, "part": { "type": "output_text", "text": "", "annotations": [] }, "sequence_number": 5 }),
        json!({ "type": "response.output_text.delta", "item_id": "msg_1", "output_index": 1, "content_index": 0, "delta": "visible", "sequence_number": 6 }),
        json!({ "type": "response.output_text.done", "item_id": "msg_1", "output_index": 1, "content_index": 0, "text": "visible", "sequence_number": 7 }),
        json!({ "type": "response.content_part.done", "item_id": "msg_1", "output_index": 1, "content_index": 0, "part": { "type": "output_text", "text": "visible", "annotations": [] }, "sequence_number": 8 }),
        json!({ "type": "response.output_item.done", "output_index": 1, "item": response["output"][1], "sequence_number": 9 }),
        json!({ "type": "response.completed", "response": response }),
    );
    let transport = ReasoningTransport::from_continuation_key([8; 32]);
    let mut stream = SseTranscoder::new(
        Cursor::new(source.into_bytes()),
        UpstreamProtocol::Responses,
        UpstreamProtocol::AnthropicMessages,
        1024 * 1024,
        Some(&transport),
    )
    .expect("supported route");
    let mut output = String::new();
    stream
        .read_to_string(&mut output)
        .expect("converted stream");

    assert!(!output.contains("private plan"));
    assert_eq!(output.matches("event: content_block_start").count(), 2);
    assert_eq!(output.matches("event: content_block_stop").count(), 2);
    let reasoning_start = output.find("redacted_thinking").expect("opaque reasoning");
    let text_start = output.find("\"text_delta\"").expect("visible text");
    assert!(reasoning_start < text_start);
    assert!(output.contains("visible"));
    assert!(output.contains("event: message_stop"));
}

#[test]
fn anthropic_text_stream_converts_to_responses() {
    let source = format!(
            "event: message_start\ndata: {}\n\nevent: content_block_start\ndata: {}\n\nevent: content_block_delta\ndata: {}\n\nevent: content_block_stop\ndata: {}\n\nevent: message_delta\ndata: {}\n\nevent: message_stop\ndata: {}\n\n",
            json!({
                "type": "message_start",
                "message": {
                    "id": "msg_1",
                    "type": "message",
                    "role": "assistant",
                    "model": "sandbox",
                    "content": [],
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": { "input_tokens": 2, "output_tokens": 0 }
                }
            }),
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "text", "text": "" }
            }),
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "text_delta", "text": "hello" }
            }),
            json!({ "type": "content_block_stop", "index": 0 }),
            json!({
                "type": "message_delta",
                "delta": { "stop_reason": "end_turn", "stop_sequence": null },
                "usage": { "input_tokens": 2, "output_tokens": 1 }
            }),
            json!({ "type": "message_stop" }),
        );
    let mut stream = SseTranscoder::new(
        Cursor::new(source.into_bytes()),
        UpstreamProtocol::AnthropicMessages,
        UpstreamProtocol::Responses,
        1024 * 1024,
        None,
    )
    .expect("supported route");
    let mut output = String::new();
    stream
        .read_to_string(&mut output)
        .expect("converted stream");

    assert!(output.contains("response.created"));
    assert!(output.contains("response.output_text.delta"), "{output}");
    assert!(output.contains("hello"));
    assert!(output.contains("response.completed"));
    assert!(output.contains("\"input_tokens\":2"));
    assert!(output.contains("\"output_tokens\":1"));
}

#[test]
fn anthropic_custom_tool_stream_restores_codex_custom_events() {
    let name = crate::gateway::transform::tool_names::render_target_name(
        UpstreamProtocol::AnthropicMessages,
        None,
        "apply_patch",
        crate::gateway::transform::ToolKind::Custom,
    )
    .expect("encode custom tool");
    let arguments = json!({ "input": "*** Begin Patch\n*** End Patch" }).to_string();
    let source = format!(
        "event: message_start\ndata: {}\n\nevent: content_block_start\ndata: {}\n\nevent: content_block_delta\ndata: {}\n\nevent: content_block_stop\ndata: {}\n\nevent: message_delta\ndata: {}\n\nevent: message_stop\ndata: {}\n\n",
        json!({"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","model":"sandbox","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":0}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_1","name":name,"input":{}}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":arguments}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"input_tokens":1,"output_tokens":1}}),
        json!({"type":"message_stop"}),
    );
    let mut stream = SseTranscoder::new(
        Cursor::new(source.into_bytes()),
        UpstreamProtocol::AnthropicMessages,
        UpstreamProtocol::Responses,
        1024 * 1024,
        None,
    )
    .expect("supported route");
    let mut output = String::new();
    stream
        .read_to_string(&mut output)
        .expect("converted stream");
    assert!(output.contains("response.custom_tool_call_input.delta"));
    assert!(output.contains("response.custom_tool_call_input.done"));
    assert!(output.contains("\"type\":\"custom_tool_call\""));
    assert!(output.contains("\"input\":\"*** Begin Patch\\n*** End Patch\""));
    assert!(!output.contains("response.function_call_arguments.delta"));
}

#[test]
fn nvidia_reasoning_stream_is_opaque_for_both_clients() {
    let source = format!(
        "data: {}\n\ndata: {}\n\ndata: {}\n\ndata: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({
            "id": "chat_1",
            "model": "sandbox",
            "usage": null,
            "choices": [{
                "index": 0,
                "delta": { "role": "assistant" },
                "finish_reason": null
            }]
        }),
        json!({
            "id": "chat_1",
            "model": "sandbox",
            "choices": [{
                "index": 0,
                "delta": { "reasoning_content": "private reasoning" },
                "finish_reason": null
            }]
        }),
        json!({
            "id": "chat_1",
            "model": "sandbox",
            "choices": [{
                "index": 0,
                "delta": { "content": "visible answer" },
                "finish_reason": null
            }]
        }),
        json!({
            "id": "chat_1",
            "model": "sandbox",
            "choices": [{ "index": 0, "delta": {}, "finish_reason": "stop" }]
        }),
        json!({
            "id": "chat_1",
            "model": "sandbox",
            "choices": [],
            "usage": {
                "prompt_tokens": 2,
                "completion_tokens": 3,
                "total_tokens": 5,
                "prompt_tokens_details": { "cached_tokens": 1 },
                "completion_tokens_details": { "reasoning_tokens": 2 }
            }
        }),
    );
    let transport = ReasoningTransport::from_continuation_key([7; 32]);
    for target in [
        UpstreamProtocol::Responses,
        UpstreamProtocol::AnthropicMessages,
    ] {
        let mut stream = SseTranscoder::new(
            Cursor::new(source.clone().into_bytes()),
            UpstreamProtocol::ChatCompletions,
            target,
            1024 * 1024,
            Some(&transport),
        )
        .expect("supported route");
        let mut output = String::new();
        stream
            .read_to_string(&mut output)
            .expect("converted stream");
        assert!(!output.contains("private reasoning"));
        assert!(output.contains("visible answer"));
        match target {
            asb_core::UpstreamProtocol::GeminiGenerateContent => {
                panic!("Google native has a separate Claude fixture")
            }
            UpstreamProtocol::Responses => assert!(output.contains("encrypted_content")),
            UpstreamProtocol::AnthropicMessages => {
                assert!(output.contains("redacted_thinking"))
            }
            UpstreamProtocol::ChatCompletions => unreachable!(),
        }
    }
}
