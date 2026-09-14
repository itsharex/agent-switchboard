use super::*;
use crate::gateway::transform::{convert_request, convert_response};
use asb_core::UpstreamProtocol::{AnthropicMessages as Claude, GeminiGenerateContent as Google};

fn transport() -> ReasoningTransport {
    ReasoningTransport::from_continuation_key([5; 32])
}
fn request(value: Value) -> Result<Value, TransformError> {
    let result = convert_request(
        Claude,
        Google,
        &serde_json::to_vec(&value).unwrap(),
        None,
        Some(&transport()),
        None,
    )?;
    Ok(serde_json::from_slice(&result.body).unwrap())
}
fn reply(value: Value) -> Value {
    serde_json::from_slice(
        &convert_response(
            Google,
            Claude,
            &serde_json::to_vec(&value).unwrap(),
            Some(&transport()),
        )
        .unwrap(),
    )
    .unwrap()
}
fn upstream(parts: Value) -> Value {
    json!({"responseId":"google-response","modelVersion":"gemini-3.1-pro","candidates":[{"index":0,"content":{"role":"model","parts":parts},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":100,"cachedContentTokenCount":60,"candidatesTokenCount":12,"thoughtsTokenCount":8,"totalTokenCount":120}})
}
fn source(messages: Value) -> Value {
    json!({"model":"gemini-3.1-pro","stream":false,"max_tokens":1024,"messages":messages})
}

#[test]
fn media_tools_and_generation_keep_their_semantics() {
    let mut body = source(json!([
        {"role":"assistant","content":[{"type":"tool_use","id":"read","name":"Read","input":{}}]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"read","is_error":true,"content":[{"type":"text","text":"failed"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"aGVsbG8="}}]}]}
    ]));
    body["tools"] =
        json!([{"name":"Read","input_schema":{"type":"object","additionalProperties":false}}]);
    body["tool_choice"] = json!({"type":"tool","name":"Read"});
    body["system"] =
        json!([{"type":"text","text":"system","cache_control":{"type":"ephemeral","ttl":"5m"}}]);
    body["metadata"] = json!({"user_id":"local-user"});
    body["output_config"] = json!({"effort":"max"});
    body["top_k"] = json!(30);
    let rendered = request(body).unwrap();
    assert_eq!(rendered["systemInstruction"]["parts"][0]["text"], "system");
    assert_eq!(
        rendered["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "high"
    );
    assert_eq!(rendered["generationConfig"]["topK"], 30);
    assert_eq!(
        rendered["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"]
            ["additionalProperties"],
        false
    );
    assert_eq!(
        rendered["toolConfig"]["functionCallingConfig"]["allowedFunctionNames"][0],
        "Read"
    );
    let result = &rendered["contents"][1]["parts"][0]["functionResponse"];
    assert_eq!(result["response"]["error"], "failed");
    assert_eq!(result["parts"][0]["inlineData"]["mimeType"], "image/png");
    assert!(rendered.get("model").is_none());
    assert!(rendered.get("stream").is_none());
    assert!(!rendered.to_string().contains("cache_control"));
}

#[test]
fn json_tool_roundtrip_preserves_parts_signatures_and_distinct_missing_ids() {
    let raw = json!([
        {"text":"private thought","thought":true,"thoughtSignature":"thinking-signature"},
        {"text":"Let me read."},
        {"functionCall":{"name":"Read","args":{"path":"a"}},"thoughtSignature":"sig-a"},
        {"functionCall":{"name":"Read","args":{"path":"b"}},"thoughtSignature":"sig-b"}
    ]);
    let response = reply(upstream(raw.clone()));
    assert_eq!(response["usage"]["input_tokens"], 40);
    assert_eq!(response["usage"]["cache_read_input_tokens"], 60);
    assert_eq!(response["usage"]["output_tokens"], 20);
    assert_eq!(response["stop_reason"], "tool_use");
    let content = response["content"].as_array().unwrap();
    assert!(!response.to_string().contains("private thought"));
    assert!(!response.to_string().contains("sig-a"));
    let calls = content
        .iter()
        .filter(|p| p["type"] == "tool_use")
        .collect::<Vec<_>>();
    assert_ne!(calls[0]["id"], calls[1]["id"]);
    let next = request(source(json!([
        {"role":"user","content":"read both"},{"role":"assistant","content":content},
        {"role":"user","content":[
            {"type":"tool_result","tool_use_id":calls[1]["id"],"content":"second","is_error":true},
            {"type":"tool_result","tool_use_id":calls[0]["id"],"content":"first"}]}
    ])))
    .unwrap();
    assert_eq!(next["contents"][1]["parts"], raw);
    let results = next["contents"][2]["parts"].as_array().unwrap();
    assert_eq!(
        results[0]["functionResponse"]["response"]["output"],
        "first"
    );
    assert_eq!(
        results[1]["functionResponse"]["response"]["error"],
        "second"
    );
    assert!(results[0]["functionResponse"].get("id").is_none());
    assert!(!next.to_string().contains("asb_gemini_"));
    assert!(!next.to_string().contains("asb-reasoning-"));
}

#[test]
fn native_call_ids_roundtrip_without_invention() {
    let raw = json!([{"functionCall":{"id":"upstream-call","name":"Read","args":{}},"thoughtSignature":"sig"}]);
    let response = reply(upstream(raw.clone()));
    let next=request(source(json!([
        {"role":"assistant","content":response["content"]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"upstream-call","content":"bad","is_error":true}]}
    ]))).unwrap();
    assert_eq!(next["contents"][0]["parts"], raw);
    assert_eq!(
        next["contents"][1]["parts"][0]["functionResponse"]["id"],
        "upstream-call"
    );
}

#[test]
fn signed_history_tampering_and_wrong_route_fail_before_upstream() {
    let response = reply(upstream(
        json!([{"text":"original","thoughtSignature":"signature"}]),
    ));
    let mut input = source(
        json!([{"role":"assistant","content":response["content"]},{"role":"user","content":"continue"}]),
    );
    let wrong = ReasoningTransport::from_continuation_key([6; 32]);
    assert!(convert_request(
        Claude,
        Google,
        &serde_json::to_vec(&input).unwrap(),
        None,
        Some(&wrong),
        None
    )
    .is_err());
    input["messages"][0]["content"][1]["text"] = json!("changed");
    assert!(request(input).err().unwrap().0.contains("已被修改"));
}

#[test]
fn explicit_budget_and_serial_policy_are_not_silently_changed() {
    let mut value = source(json!([{"role":"user","content":"hello"}]));
    value["model"] = json!("gemini-2.5-pro");
    value["thinking"] = json!({"type":"enabled","budget_tokens":4096});
    assert_eq!(
        request(value.clone()).unwrap()["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        4096
    );
    value["tools"] = json!([{"name":"Read","input_schema":{"type":"object"}}]);
    value["tool_choice"] = json!({"type":"auto","disable_parallel_tool_use":true});
    assert!(request(value)
        .err()
        .unwrap()
        .0
        .contains("disable_parallel_tool_use"));
    assert!(convert_request(
        asb_core::UpstreamProtocol::Responses,
        Google,
        b"{}",
        None,
        Some(&transport()),
        None
    )
    .is_err());
}

#[path = "stream_tests.rs"]
mod streaming;
