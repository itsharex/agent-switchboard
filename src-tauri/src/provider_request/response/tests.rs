use super::*;
use crate::provider_request::tests::{success_body, TEST_KEY};
use serde_json::json;

fn decode(protocol: UpstreamProtocol, body: Value) -> Result<Reply, String> {
    parse(protocol, body.to_string().as_bytes())
}

#[test]
fn each_protocol_requires_actual_text_and_reports_only_the_returned_model() {
    for protocol in [
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::AnthropicMessages,
    ] {
        let mut body = success_body(protocol, "连接成功。");
        let reply = decode(protocol, body.clone()).unwrap();
        assert_eq!(reply.text, "连接成功。");
        assert_eq!(reply.model.as_deref(), Some("actual-model"));
        body.as_object_mut().unwrap().remove("model");
        assert_eq!(decode(protocol, body).unwrap().model, None);
        assert!(decode(protocol, success_body(protocol, " \n\t")).is_err());
    }
}

#[test]
fn html_error_objects_and_wrong_protocol_responses_never_succeed() {
    for protocol in [
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::AnthropicMessages,
    ] {
        assert!(parse(protocol, b"<html>HTTP 200</html>").is_err());
        assert!(decode(protocol, json!({"error":{"message":"API error"}})).is_err());
        assert!(decode(protocol, json!({"message":"connected"})).is_err());
    }
    assert!(decode(
        UpstreamProtocol::Responses,
        success_body(UpstreamProtocol::ChatCompletions, "reply")
    )
    .is_err());
}

#[test]
fn incomplete_and_reasoning_only_results_are_not_successes() {
    let mut responses = success_body(UpstreamProtocol::Responses, "partial");
    responses["status"] = json!("incomplete");
    let error = decode(UpstreamProtocol::Responses, responses)
        .err()
        .unwrap();
    assert!(error.contains("1024"));
    assert!(error.contains("推理预算"));
    let reasoning = json!({"object":"response", "status":"completed",
        "output":[{"type":"reasoning", "summary":[{"type":"summary_text", "text":"thinking"}]}]});
    assert!(decode(UpstreamProtocol::Responses, reasoning).is_err());
    let mut chat = success_body(UpstreamProtocol::ChatCompletions, "partial");
    chat["choices"][0]["finish_reason"] = json!("length");
    assert!(decode(UpstreamProtocol::ChatCompletions, chat).is_err());
    let chat_reasoning = json!({"object":"chat.completion", "choices":[{"finish_reason":"stop",
        "message":{"role":"assistant", "content":null, "reasoning_content":"thinking"}}]});
    assert!(decode(UpstreamProtocol::ChatCompletions, chat_reasoning).is_err());
    let mut anthropic = success_body(UpstreamProtocol::AnthropicMessages, "partial");
    anthropic["stop_reason"] = json!("max_tokens");
    assert!(decode(UpstreamProtocol::AnthropicMessages, anthropic).is_err());
    let anthropic_reasoning = json!({"type":"message", "role":"assistant", "stop_reason":"end_turn",
        "content":[{"type":"thinking", "thinking":"still thinking"}]});
    assert!(decode(UpstreamProtocol::AnthropicMessages, anthropic_reasoning).is_err());
}

#[test]
fn credentials_are_scrubbed_inside_json_punctuation_and_plain_reply() {
    for message in [
        format!("API key '{TEST_KEY}' invalid"),
        format!("{{\"key\":\"{TEST_KEY}\"}}"),
        format!("reply: {TEST_KEY}."),
    ] {
        let sanitized = redact(message, TEST_KEY);
        assert!(!sanitized.contains(TEST_KEY));
        assert!(sanitized.contains(asb_core::redact::REDACTED));
    }
}

#[test]
fn reply_redaction_preserves_model_ids_and_removes_complete_credentials() {
    let key = "isolated!".repeat(400);
    let value = json!({"error":{"message":format!("rejected '{key}'")}});
    let sanitized = redact(error_message(&value).unwrap(), &key);
    assert!(!sanitized.contains("isolated!"));
    assert!(sanitized.contains(asb_core::redact::REDACTED));
    let model = "anthropic/claude-opus-4-6-20260210".to_string();
    assert_eq!(redact(model.clone(), TEST_KEY), model);
}
