use super::*;
use asb_core::contracts::{
    CodexChatEffortMode, CodexChatEffortParameter, CodexChatReasoning, CodexChatThinkingParameter,
};

#[test]
fn codex_explicit_chat_dialect_is_not_overridden_by_claude_model_inference() {
    let input = json!({
        "model": "gpt-5.4", "input": "hello",
        "reasoning": { "effort": "max", "summary": "auto" },
    });
    let configuration = CodexChatReasoning::Configured {
        thinking_parameter: CodexChatThinkingParameter::Thinking,
        effort_parameter: CodexChatEffortParameter::ReasoningEffort,
        effort_mode: CodexChatEffortMode::DeepSeek,
    };
    let converted = convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        &serde_json::to_vec(&input).unwrap(),
        None,
        None,
        Some(&configuration),
    )
    .unwrap();
    let output: Value = serde_json::from_slice(&converted.body).unwrap();
    assert_eq!(output["reasoning_effort"], "max");
    assert_eq!(output["thinking"]["type"], "enabled");
    assert_eq!(output["messages"][0]["content"], "hello");
    assert!(output["messages"][0].get("reasoning_content").is_none());
}

#[test]
fn codex_effort_still_requires_an_explicit_chat_dialect() {
    let input =
        json!({ "model": "gpt-5.4", "input": "hello", "reasoning": { "effort": "medium" } });
    let result = convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        &serde_json::to_vec(&input).unwrap(),
        None,
        None,
        None,
    );
    let error = result
        .err()
        .expect("Claude effort support must not bypass Codex configuration");
    assert!(error.0.contains("reasoning.effort"));
}

#[test]
fn codex_text_tool_outputs_keep_their_existing_chat_representation() {
    let input = json!({
        "model": "sandbox-model",
        "input": [
            { "type": "function_call", "call_id": "first", "name": "capture", "arguments": "{}" },
            { "type": "function_call", "call_id": "second", "name": "capture", "arguments": "{}" },
            { "type": "function_call_output", "call_id": "first", "output": [
                { "type": "input_text", "text": "one" }, { "type": "input_text", "text": "two" },
            ] },
            { "type": "function_call_output", "call_id": "second", "output": "three" },
        ],
    });
    let converted = convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        &serde_json::to_vec(&input).unwrap(),
        None,
        None,
        None,
    )
    .unwrap();
    let output: Value = serde_json::from_slice(&converted.body).unwrap();
    assert_eq!(output["messages"].as_array().unwrap().len(), 3);
    assert_eq!(
        output["messages"][1],
        json!({ "role": "tool", "tool_call_id": "first", "content": "onetwo" })
    );
    assert_eq!(
        output["messages"][2],
        json!({ "role": "tool", "tool_call_id": "second", "content": "three" })
    );
}

#[test]
fn chat_tool_history_can_enter_responses_without_losing_the_failure_marker() {
    let mut input = request("gpt-5.4");
    input["messages"].as_array_mut().unwrap().extend([
        json!({ "role": "assistant", "content": [tool_call("capture-1")] }),
        json!({ "role": "user", "content": [{
            "type": "tool_result", "tool_use_id": "capture-1", "is_error": true, "content": "failed",
        }] }),
    ]);
    let chat = convert(&input, UpstreamProtocol::ChatCompletions).unwrap();
    let converted = convert_request(
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::Responses,
        &serde_json::to_vec(&chat).unwrap(),
        None,
        None,
        None,
    )
    .unwrap();
    let output: Value = serde_json::from_slice(&converted.body).unwrap();
    let result = output["input"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["type"] == "function_call_output")
        .unwrap();
    assert_eq!(result["call_id"], "capture-1");
    assert_eq!(
        result["output"][0]["text"],
        format!("{TOOL_RESULT_ERROR_MARKER}\nfailed")
    );
}

#[test]
fn native_anthropic_requests_bypass_cross_protocol_normalization() {
    let body = br#"{ "model":"claude-sonnet-4-6", "stream":true,
        "thinking":{"type":"enabled","budget_tokens":8000},
        "messages":[{"role":"user","content":[{"type":"text","text":"hello",
        "cache_control":{"type":"ephemeral","ttl":"1h"}}]}] }"#;
    let converted = convert_request(
        UpstreamProtocol::AnthropicMessages,
        UpstreamProtocol::AnthropicMessages,
        body,
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(converted.body, body);
    assert!(converted.stream);
}
