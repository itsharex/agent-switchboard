//! Reasoning replay coverage for cross-protocol request conversion.

use super::*;
use serde_json::json;

#[test]
fn replayed_reasoning_reaches_an_anthropic_upstream_in_its_own_shape() {
    let transport = ReasoningTransport::from_continuation_key([21; 32]);
    let signed = transport
        .from_anthropic_thinking("private plan".to_string(), Some("sig-1".to_string()))
        .expect("seal signed thinking");
    let opaque = transport
        .from_redacted("opaque-upstream-blob".to_string())
        .expect("seal an upstream opaque block");
    let unsigned = transport
        .from_chat_content("chat plan".to_string())
        .expect("seal unsigned reasoning");
    let input = json!({
        "model": "sandbox-model",
        "max_output_tokens": 128,
        "input": [
            { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "hello" }] },
            { "type": "reasoning", "summary": [], "encrypted_content": signed.continuation },
            { "type": "reasoning", "summary": [], "encrypted_content": opaque.continuation },
            { "type": "reasoning", "summary": [], "encrypted_content": unsigned.continuation },
            { "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "answer" }] }
        ]
    });
    let converted = super::convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::AnthropicMessages,
        &serde_json::to_vec(&input).expect("encode request"),
        None,
        Some(&transport),
        None,
    )
    .expect("replay reasoning to an Anthropic upstream");
    let body = String::from_utf8(converted.body).expect("Anthropic JSON is UTF-8");
    // The upstream receives its own block shapes, never this gateway's payload.
    assert!(
        body.contains(r#"{"type":"thinking","thinking":"private plan","signature":"sig-1"}"#),
        "signed thinking block missing: {body}"
    );
    assert!(
        body.contains(r#""type":"redacted_thinking","data":"opaque-upstream-blob""#),
        "opaque block missing: {body}"
    );
    assert!(
        body.contains(r#""thinking":"chat plan""#),
        "unsigned thinking block missing: {body}"
    );
    assert!(
        !body.contains("asb-reasoning-v3."),
        "the client-facing continuation must never reach the upstream"
    );
}

#[test]
fn an_opaque_block_cannot_enter_chat_history() {
    let transport = ReasoningTransport::from_continuation_key([22; 32]);
    let opaque = transport
        .from_redacted("opaque-upstream-blob".to_string())
        .expect("seal an upstream opaque block");
    let input = json!({
        "model": "sandbox-model",
        "input": [
            { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "hello" }] },
            { "type": "reasoning", "summary": [], "encrypted_content": opaque.continuation },
            { "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "answer" }] }
        ]
    });
    let error = super::convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        &serde_json::to_vec(&input).expect("encode request"),
        None,
        Some(&transport),
        None,
    )
    .err()
    .expect("an unreadable block must not be dropped from Chat history");
    assert!(error.0.contains("不透明上游块"), "{}", error.0);
}
