//! Streaming coverage for upstream Anthropic thinking blocks.

use super::*;
use serde_json::json;
use std::io::{Cursor, Read};

const SOURCE_LIMIT: u64 = 1024 * 1024;

#[test]
fn anthropic_thinking_stream_stays_opaque_and_precedes_the_message() {
    let mut output = String::new();
    transcoded(
        &[
            ("message_start", message_start()),
            (
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": 0,
                    "content_block": { "type": "thinking", "thinking": "", "signature": "" }
                }),
            ),
            (
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": { "type": "thinking_delta", "thinking": "private reasoning" }
                }),
            ),
            (
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": { "type": "signature_delta", "signature": "opaque-signature" }
                }),
            ),
            (
                "content_block_stop",
                json!({ "type": "content_block_stop", "index": 0 }),
            ),
            (
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": 1,
                    "content_block": { "type": "text", "text": "" }
                }),
            ),
            (
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": 1,
                    "delta": { "type": "text_delta", "text": "visible answer" }
                }),
            ),
            (
                "content_block_stop",
                json!({ "type": "content_block_stop", "index": 1 }),
            ),
            (
                "message_delta",
                json!({
                    "type": "message_delta",
                    "delta": { "stop_reason": "end_turn", "stop_sequence": null },
                    "usage": { "output_tokens": 4 }
                }),
            ),
            ("message_stop", json!({ "type": "message_stop" })),
        ],
        Some(&ReasoningTransport::from_continuation_key([13; 32])),
    )
    .read_to_string(&mut output)
    .expect("converted stream");
    assert!(
        !output.contains("private reasoning"),
        "the readable reasoning trace must not reach the client"
    );
    assert!(output.contains("visible answer"));
    let reasoning = output
        .find("asb-reasoning-v3.")
        .expect("opaque reasoning continuation");
    let message = output
        .find("visible answer")
        .expect("visible assistant text");
    assert!(
        reasoning < message,
        "the reasoning item must precede the assistant message"
    );
}

#[test]
fn anthropic_thinking_stream_requires_the_continuation_channel() {
    let mut output = String::new();
    transcoded(
        &[
            ("message_start", message_start()),
            (
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": 0,
                    "content_block": { "type": "thinking", "thinking": "" }
                }),
            ),
            (
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": { "type": "thinking_delta", "thinking": "private reasoning" }
                }),
            ),
            (
                "content_block_stop",
                json!({ "type": "content_block_stop", "index": 0 }),
            ),
            ("message_stop", json!({ "type": "message_stop" })),
        ],
        None,
    )
    .read_to_string(&mut output)
    .expect("conversion failure is reported inside the stream");
    assert!(output.contains("无法转换上游 SSE"));
    assert!(!output.contains("private reasoning"));
    assert!(!output.contains("response.completed"));
}

#[test]
fn anthropic_stream_rejects_a_malformed_thinking_delta() {
    let mut output = String::new();
    transcoded(
        &[
            ("message_start", message_start()),
            (
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": 0,
                    "content_block": { "type": "thinking", "thinking": "" }
                }),
            ),
            (
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": { "type": "thinking_delta", "thinking": 7 }
                }),
            ),
        ],
        Some(&ReasoningTransport::from_continuation_key([13; 32])),
    )
    .read_to_string(&mut output)
    .expect("conversion failure is reported inside the stream");
    assert!(output.contains("无法转换上游 SSE"));
    assert!(!output.contains("response.completed"));
}

fn message_start() -> Value {
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
            "usage": { "input_tokens": 2, "output_tokens": 1 }
        }
    })
}

fn transcoded(
    frames: &[(&str, Value)],
    transport: Option<&ReasoningTransport>,
) -> SseTranscoder<Cursor<Vec<u8>>> {
    let source = frames
        .iter()
        .map(|(event, value)| format!("event: {event}\ndata: {value}\n\n"))
        .collect::<String>();
    SseTranscoder::new(
        Cursor::new(source.into_bytes()),
        UpstreamProtocol::AnthropicMessages,
        UpstreamProtocol::Responses,
        SOURCE_LIMIT,
        transport,
    )
    .expect("supported route")
}
