//! Exercise the public reader as well as the Responses-specific state machine.

use super::*;
use crate::gateway::transform::SseTranscoder;
use std::io::{self, Cursor, Read};

struct Bytewise(Cursor<Vec<u8>>);

impl Read for Bytewise {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        self.0.read(&mut buffer[..1])
    }
}

fn source(events: Vec<Value>) -> String {
    events
        .into_iter()
        .map(|value| {
            format!(
                "event: {}\ndata: {value}\n\n",
                value["type"].as_str().unwrap()
            )
        })
        .collect()
}

fn transcode(source: String) -> (Vec<Value>, bool) {
    let mut reader = SseTranscoder::new(
        Bytewise(Cursor::new(source.into_bytes())),
        UpstreamProtocol::Responses,
        UpstreamProtocol::AnthropicMessages,
        1024 * 1024,
        Some(&transport()),
    )
    .unwrap();
    let mut text = String::new();
    reader.read_to_string(&mut text).unwrap();
    let events = text
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|data| serde_json::from_str(data).unwrap())
        .collect();
    (events, reader.failed())
}

#[test]
fn public_reader_routes_token_limit_incomplete_to_the_responses_converter() {
    let mut item = text_item("msg_text", "answer \u{4f60}\u{597d}");
    item["status"] = json!("incomplete");
    let value = limited(vec![item.clone()]);
    let body = source(vec![
        json!({"type":"response.created","response":{"id":"resp_lifecycle","model":"test-model"}}),
        added(0, item.clone()),
        text_delta(0, "msg_text", "answer "),
        done(0, item),
        json!({"type":"response.incomplete","response":value}),
    ]);
    let (events, failed) = transcode(body);
    assert!(
        !failed,
        "a token-limit response must not be intercepted as an upstream error: {events:?}"
    );
    let mut harness = Harness::new();
    harness.events = events;
    assert_success(&harness, &value);
}

#[test]
fn public_reader_validates_complete_finish_done_marker_and_frame_truncation() {
    let value = response(vec![text_item("msg_text", "complete")]);
    let body = source(vec![json!({"type":"response.completed","response":value})]);
    for suffix in ["", "data: [DONE]\n\n"] {
        let (events, failed) = transcode(format!("{body}{suffix}"));
        assert!(!failed, "{events:?}");
        let mut harness = Harness::new();
        harness.events = events;
        assert_success(&harness, &value);
    }
    let truncated = body.trim_end().to_string();
    let (events, failed) = transcode(truncated);
    assert!(failed);
    assert!(of_type(&events, "message_stop").is_empty());
}

#[test]
fn public_reader_rejects_failed_cancelled_and_non_token_incomplete() {
    for (kind, status, reason) in [
        ("response.failed", "failed", Value::Null),
        ("response.cancelled", "cancelled", Value::Null),
        ("response.completed", "failed", Value::Null),
        ("response.incomplete", "incomplete", json!("content_filter")),
    ] {
        let mut value = response(vec![]);
        value["status"] = json!(status);
        if !reason.is_null() {
            value["incomplete_details"] = json!({"reason":reason});
        }
        let (events, failed) = transcode(source(vec![json!({"type":kind,"response":value})]));
        assert!(failed, "{kind}");
        assert_eq!(of_type(&events, "error").len(), 1);
        assert!(of_type(&events, "message_stop").is_empty());
    }
}
