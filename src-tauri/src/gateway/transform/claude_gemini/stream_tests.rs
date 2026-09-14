use super::*;
use crate::gateway::transform::SseTranscoder;
use std::io::Read;

fn event(value: Value) -> String {
    format!("data: {value}\n\n")
}
fn transcode(raw: &str) -> (String, crate::gateway::usage_metadata::TokenUsage) {
    let mut stream = SseTranscoder::new(
        raw.as_bytes(),
        Google,
        Claude,
        1_000_000,
        Some(&transport()),
    )
    .unwrap();
    let mut output = String::new();
    stream.read_to_string(&mut output).unwrap();
    (output, stream.usage())
}
fn frames(output: &str) -> Vec<Value> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|value| serde_json::from_str(value).unwrap())
        .collect()
}
struct StepReader {
    chunks: Vec<Vec<u8>>,
}
impl Read for StepReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.chunks.is_empty() {
            return Ok(0);
        }
        let chunk = self.chunks.remove(0);
        out[..chunk.len()].copy_from_slice(&chunk);
        Ok(chunk.len())
    }
}

#[test]
fn streaming_is_incremental_and_late_usage_includes_thinking_and_cache() {
    let first = event(
        json!({"responseId":"s","modelVersion":"gemini-3.1-pro","candidates":[{"content":{"role":"model","parts":[{"text":"hello"}]}}]}),
    );
    let last = event(
        json!({"responseId":"s","candidates":[{"content":{"parts":[{"text":" world","thoughtSignature":"signed"}]},"finishReason":"STOP"}]}),
    );
    let usage = event(
        json!({"usageMetadata":{"promptTokenCount":20,"cachedContentTokenCount":7,"candidatesTokenCount":4,"thoughtsTokenCount":3}}),
    );
    let raw = format!("{first}{last}{usage}");
    let mut stream = SseTranscoder::new(
        StepReader {
            chunks: vec![first.into_bytes(), last.into_bytes(), usage.into_bytes()],
        },
        Google,
        Claude,
        1_000_000,
        Some(&transport()),
    )
    .unwrap();
    let mut first_out = [0; 8192];
    let n = stream.read(&mut first_out).unwrap();
    let early = String::from_utf8_lossy(&first_out[..n]);
    assert!(early.contains("hello"));
    assert!(!early.contains("message_stop"));
    let (output, usage) = transcode(&raw);
    assert!(output.contains("message_stop"));
    assert!(!output.contains("\"data\":\"signed\""));
    assert_eq!(usage.input_tokens, Some(20));
    assert_eq!(usage.output_tokens, Some(7));
    assert_eq!(usage.cache_read_tokens, Some(7));
    assert_eq!(usage.reasoning_tokens, Some(3));
    let events = frames(&output);
    let delta = events
        .iter()
        .find(|v| v["type"] == "message_delta")
        .unwrap();
    assert_eq!(delta["usage"]["input_tokens"], 13);
    assert_eq!(delta["usage"]["output_tokens"], 7);
}

#[test]
fn stream_tool_continuation_replays_original_parts_after_client_text_coalescing() {
    let parts = vec![
        json!({"text":"a"}),
        json!({"text":"b"}),
        json!({"functionCall":{"name":"Read","args":{"path":"x"}},"thoughtSignature":"sig"}),
    ];
    let mut raw = String::new();
    for part in &parts {
        raw.push_str(&event(json!({"responseId":"s","modelVersion":"gemini-3.1-pro","candidates":[{"content":{"parts":[part]}}]})));
    }
    raw.push_str(&event(json!({"candidates":[{"finishReason":"STOP"}]})));
    let (output, _) = transcode(&raw);
    let events = frames(&output);
    let start = events
        .iter()
        .find(|v| v["content_block"]["type"] == "tool_use")
        .unwrap();
    let signature = events
        .iter()
        .find(|v| v["delta"]["type"] == "signature_delta")
        .unwrap();
    let opaque =
        json!({"type":"thinking","thinking":"","signature":signature["delta"]["signature"]});
    let next=request(source(json!([
        {"role":"assistant","content":[{"type":"text","text":"ab"},{"type":"tool_use","id":start["content_block"]["id"],"name":"Read","input":{"path":"x"}},opaque]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":start["content_block"]["id"],"content":"ok"}]}
    ]))).unwrap();
    assert_eq!(next["contents"][0]["parts"], json!(parts));
}

#[test]
fn truncation_and_safety_blocks_are_errors_and_unknown_usage_is_not_zero() {
    for value in [
        json!({"candidates":[{"content":{"parts":[{"text":"partial"}]}}]}),
        json!({"promptFeedback":{"blockReason":"SAFETY"}}),
        json!({"candidates":[{"finishReason":"MALFORMED_FUNCTION_CALL"}]}),
    ] {
        let (out, usage) = transcode(&event(value));
        assert!(out.contains("event: error"));
        assert!(!out.contains("message_stop"));
        assert_eq!(usage.output_tokens, None);
    }
    let mut unknown = upstream(json!([{"text":"hello"}]));
    unknown.as_object_mut().unwrap().remove("usageMetadata");
    let parsed = super::super::response(&unknown, Some(&transport())).unwrap();
    assert_eq!(parsed.usage.input_tokens, None);
    assert_eq!(parsed.usage.output_tokens, None);
}
