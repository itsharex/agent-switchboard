use super::*;

fn convert(
    from: UpstreamProtocol,
    to: UpstreamProtocol,
    body: &[u8],
) -> Result<Vec<u8>, TransformError> {
    super::convert_response(from, to, body, None)
}

#[test]
fn anthropic_thinking_becomes_an_opaque_codex_reasoning_item() {
    let transport = ReasoningTransport::from_continuation_key([11; 32]);
    let upstream = json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "sandbox",
        "content": [
            { "type": "thinking", "thinking": "private reasoning", "signature": "opaque-signature" },
            { "type": "text", "text": "visible answer" }
        ],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": { "input_tokens": 2, "output_tokens": 3 }
    });
    let converted = super::convert_response(
        UpstreamProtocol::AnthropicMessages,
        UpstreamProtocol::Responses,
        &serde_json::to_vec(&upstream).expect("encode upstream"),
        Some(&transport),
    )
    .expect("convert Anthropic thinking response");
    assert!(
        !converted
            .windows(b"private reasoning".len())
            .any(|part| part == b"private reasoning"),
        "the readable reasoning trace must not reach the client"
    );
    let response: Value = serde_json::from_slice(&converted).expect("Responses JSON");
    assert_eq!(response["output"][0]["type"], "reasoning");
    assert_eq!(
        response["output"][1]["content"][0]["text"],
        "visible answer"
    );
    let continuation = response["output"][0]["encrypted_content"]
        .as_str()
        .expect("opaque reasoning continuation");
    assert_eq!(
        transport
            .from_continuation(continuation.to_string())
            .expect("continuation belongs to this route")
            .content,
        "private reasoning"
    );
}

#[test]
fn anthropic_thinking_without_a_continuation_channel_is_rejected() {
    let upstream = json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "sandbox",
        "content": [{ "type": "thinking", "thinking": "private reasoning", "signature": "s" }],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": { "input_tokens": 2, "output_tokens": 3 }
    });
    let error = convert(
        UpstreamProtocol::AnthropicMessages,
        UpstreamProtocol::Responses,
        &serde_json::to_vec(&upstream).expect("encode upstream"),
    )
    .expect_err("reasoning requires the backend-bound continuation channel");
    assert!(
        error.0.contains("推理续接通道"),
        "unexpected error: {}",
        error.0
    );
}

#[test]
fn anthropic_tool_response_becomes_chat_tool_call() {
    let input = br#"{
          "id":"msg_1","type":"message","role":"assistant","model":"sandbox",
          "content":[{"type":"tool_use","id":"tool_1","name":"weather","input":{"city":"Shanghai"}}],
          "stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":2,"output_tokens":3}
        }"#;
    let output = convert(
        UpstreamProtocol::AnthropicMessages,
        UpstreamProtocol::ChatCompletions,
        input,
    )
    .unwrap();
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        value["choices"][0]["message"]["tool_calls"][0]["function"]["name"],
        "weather"
    );
    assert_eq!(value["choices"][0]["finish_reason"], "tool_calls");
}

#[test]
fn encoded_chat_tool_response_restores_the_responses_namespace() {
    let name = render_target_name(
        UpstreamProtocol::ChatCompletions,
        Some("functions."),
        "apply.patch",
        ToolKind::Function,
    )
    .expect("encode namespace tool");
    let input = json!({
        "id": "chat_1",
        "object": "chat.completion",
        "model": "sandbox",
        "service_tier": "on_demand",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": { "name": name, "arguments": "{\"path\":\"a\"}" }
                }]
            },
            "finish_reason": "tool_calls",
            "logprobs": null
        }],
        "usage": { "prompt_tokens": 2, "completion_tokens": 3, "total_tokens": 5 }
    });

    let output = convert(
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::Responses,
        &serde_json::to_vec(&input).expect("encode fixture"),
    )
    .expect("convert tool response");
    let value: Value = serde_json::from_slice(&output).expect("Responses response JSON");
    assert_eq!(value["output"][0]["name"], "apply.patch");
    assert_eq!(value["output"][0]["namespace"], "functions.");
    assert_eq!(value["output"][0]["arguments"], "{\"path\":\"a\"}");
}

#[test]
fn encoded_chat_custom_tool_response_restores_free_form_input() {
    let name = render_target_name(
        UpstreamProtocol::ChatCompletions,
        None,
        "apply_patch",
        ToolKind::Custom,
    )
    .unwrap();
    let body = json!({
        "id":"chat_1","object":"chat.completion","model":"m",
        "choices":[{"index":0,"message":{"role":"assistant","content":null,
          "tool_calls":[{"id":"call_1","type":"function","function":{
            "name":name,"arguments":r#"{"input":"*** Begin Patch\n*** End Patch"}"#
          }}]},"finish_reason":"tool_calls"}],
        "usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}
    });
    let converted = convert_response(
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::Responses,
        &serde_json::to_vec(&body).unwrap(),
        None,
    )
    .unwrap();
    let value: Value = serde_json::from_slice(&converted).unwrap();
    assert_eq!(value["output"][0]["type"], "custom_tool_call");
    assert_eq!(value["output"][0]["name"], "apply_patch");
    assert_eq!(
        value["output"][0]["input"],
        "*** Begin Patch\n*** End Patch"
    );
}

#[test]
fn unknown_response_field_is_rejected_not_hidden() {
    let input = br#"{"id":"x","model":"m","choices":[],"deployment":"priority"}"#;
    let error = convert(
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::Responses,
        input,
    )
    .unwrap_err();
    assert!(error.0.contains("deployment"));
}

#[test]
fn chat_reasoning_is_opaque_to_responses_and_restores_for_the_next_turn() {
    let transport = ReasoningTransport::from_continuation_key([7; 32]);
    let upstream = json!({
        "id": "chat_1",
        "object": "chat.completion",
        "model": "sandbox",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "reasoning_content": "private reasoning",
                "content": "visible answer"
            },
            "finish_reason": "stop",
            "logprobs": null
        }],
        "usage": { "prompt_tokens": 2, "completion_tokens": 3, "total_tokens": 5 }
    });
    let converted = convert_response(
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::Responses,
        &serde_json::to_vec(&upstream).expect("encode upstream"),
        Some(&transport),
    )
    .expect("convert NVIDIA-style response");
    let response: Value = serde_json::from_slice(&converted).expect("Responses JSON");
    assert!(!converted
        .windows(b"private reasoning".len())
        .any(|part| part == b"private reasoning"));
    assert_eq!(response["output"][0]["type"], "reasoning");
    assert_eq!(response["output"][0]["content"], json!([]));

    let mut assistant_message = response["output"][1].clone();
    let assistant_message = assistant_message
        .as_object_mut()
        .expect("Responses message object");
    assistant_message.remove("id");
    assistant_message.remove("status");

    let request = json!({
        "model": "sandbox",
        "input": [
            response["output"][0].clone(),
            Value::Object(assistant_message.clone()),
            { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "continue" }] }
        ]
    });
    let replay = crate::gateway::transform::convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        &serde_json::to_vec(&request).expect("encode replay"),
        None,
        Some(&transport),
        None,
    )
    .expect("restore reasoning for the upstream request");
    let replay: Value = serde_json::from_slice(&replay.body).expect("Chat JSON");
    assert_eq!(
        replay["messages"][0]["reasoning_content"],
        "private reasoning"
    );
    assert_eq!(replay["messages"][1]["content"], "continue");
}

#[test]
fn chat_reasoning_is_opaque_to_anthropic_and_restores_for_the_next_turn() {
    let transport = ReasoningTransport::from_continuation_key([7; 32]);
    let upstream = json!({
        "id": "chat_1",
        "object": "chat.completion",
        "model": "sandbox",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "reasoning_content": "private reasoning",
                "content": "visible answer"
            },
            "finish_reason": "stop",
            "logprobs": null
        }],
        "usage": { "prompt_tokens": 2, "completion_tokens": 3, "total_tokens": 5 }
    });
    let converted = convert_response(
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::AnthropicMessages,
        &serde_json::to_vec(&upstream).expect("encode upstream"),
        Some(&transport),
    )
    .expect("convert NVIDIA-style response");
    let response: Value = serde_json::from_slice(&converted).expect("Anthropic JSON");
    assert!(!converted
        .windows(b"private reasoning".len())
        .any(|part| part == b"private reasoning"));
    assert_eq!(response["content"][0]["type"], "redacted_thinking");

    let request = json!({
        "model": "sandbox",
        "max_tokens": 64,
        "messages": [
            { "role": "assistant", "content": response["content"].clone() },
            { "role": "user", "content": "continue" }
        ]
    });
    let replay = crate::gateway::transform::convert_request(
        UpstreamProtocol::AnthropicMessages,
        UpstreamProtocol::ChatCompletions,
        &serde_json::to_vec(&request).expect("encode replay"),
        None,
        Some(&transport),
        None,
    )
    .expect("restore reasoning for the upstream request");
    let replay: Value = serde_json::from_slice(&replay.body).expect("Chat JSON");
    assert_eq!(
        replay["messages"][0]["reasoning_content"],
        "private reasoning"
    );
    assert_eq!(replay["messages"][1]["content"], "continue");
}
