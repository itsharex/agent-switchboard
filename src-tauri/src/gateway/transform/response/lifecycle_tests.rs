use super::*;

fn response(status: &str, output: Value) -> Value {
    json!({"id":"resp_lifecycle","object":"response","model":"test-model",
        "status":status,"output":output,"usage":{"input_tokens":12,"output_tokens":5,
        "input_tokens_details":{"cached_tokens":7}},"error":null})
}

fn text() -> Value {
    json!({"type":"message","id":"msg_text","role":"assistant","status":"completed",
        "content":[{"type":"output_text","text":"answer","annotations":[]}]})
}

fn tool(arguments: &str) -> Value {
    json!({"type":"function_call","id":"fc_tool","call_id":"call_tool","name":"Read",
        "status":"completed","arguments":arguments})
}

fn convert(value: &Value) -> Result<Value, TransformError> {
    let transport = ReasoningTransport::from_continuation_key([91; 32]);
    let bytes = convert_response(
        UpstreamProtocol::Responses,
        UpstreamProtocol::AnthropicMessages,
        &serde_json::to_vec(value).unwrap(),
        Some(&transport),
    )?;
    Ok(serde_json::from_slice(&bytes).unwrap())
}

fn limited(output: Value, reason: &str) -> Value {
    let mut value = response("incomplete", output);
    value["incomplete_details"] = json!({"reason":reason});
    value
}

#[test]
fn responses_token_limit_takes_precedence_over_tool_use_and_keeps_metering() {
    for reason in ["max_output_tokens", "max_tokens"] {
        for output in [
            json!([]),
            json!([text()]),
            json!([tool(r#"{"path":"fixture"}"#)]),
        ] {
            let message = convert(&limited(output, reason)).unwrap();
            assert_eq!(message["stop_reason"], "max_tokens");
            assert_eq!(
                message["usage"],
                json!({"input_tokens":5,"output_tokens":5,"cache_read_input_tokens":7})
            );
        }
    }
}

#[test]
fn responses_completed_text_and_tools_keep_their_normal_stop_reasons() {
    for (output, stop) in [
        (json!([text()]), "end_turn"),
        (json!([tool("{}")]), "tool_use"),
    ] {
        let mut value = response("completed", output);
        value["incomplete_details"] = Value::Null;
        assert_eq!(convert(&value).unwrap()["stop_reason"], stop);
    }
}

#[test]
fn responses_failed_cancelled_and_nonterminal_json_never_become_successes() {
    for status in [
        "failed",
        "cancelled",
        "in_progress",
        "queued",
        "unknown",
        "",
    ] {
        let error = convert(&response(status, json!([text()]))).unwrap_err();
        assert!(error.0.contains(status), "{}", error.0);
    }
    for status in [Value::Null, json!(false), json!(4)] {
        let mut value = response("completed", json!([text()]));
        value["status"] = status;
        assert!(convert(&value).is_err());
    }
    let mut value = response("completed", json!([text()]));
    value.as_object_mut().unwrap().remove("status");
    assert!(convert(&value).is_err());
}

#[test]
fn responses_incomplete_requires_an_explicit_token_limit_reason() {
    for details in [
        Value::Null,
        json!({}),
        json!("max_output_tokens"),
        json!({"reason":null}),
        json!({"reason":"content_filter"}),
        json!({"reason":"interrupted"}),
        json!({"reason":7}),
    ] {
        let mut value = response("incomplete", json!([text()]));
        value["incomplete_details"] = details;
        assert!(convert(&value).is_err(), "{value}");
    }
    assert!(convert(&response("incomplete", json!([text()]))).is_err());
}

#[test]
fn responses_completed_with_incomplete_details_is_a_conflicting_terminal() {
    let mut value = limited(json!([text()]), "max_output_tokens");
    value["status"] = json!("completed");
    assert!(convert(&value).is_err());
}

#[test]
fn responses_error_envelopes_preserve_provider_details_on_every_status() {
    for status in ["completed", "incomplete", "failed", "cancelled"] {
        let mut value = limited(json!([text()]), "max_output_tokens");
        value["status"] = json!(status);
        value["error"] = json!({"code":"quota_exceeded","message":"fixture quota exhausted"});
        let error = convert(&value).unwrap_err();
        assert!(error.0.contains("quota_exceeded"), "{}", error.0);
        assert!(error.0.contains("fixture quota exhausted"), "{}", error.0);
    }
    let mut value = response("completed", json!([]));
    value["error"] = json!("fixture failure");
    assert!(convert(&value).unwrap_err().0.contains("fixture failure"));
}

#[test]
fn responses_truncated_tool_arguments_are_not_replaced_with_an_empty_object() {
    for arguments in [r#"{"path":"#, "", "null", "[]", "3", r#""text""#] {
        assert!(convert(&limited(json!([tool(arguments)]), "max_output_tokens")).is_err());
        assert!(convert(&response("completed", json!([tool(arguments)]))).is_err());
    }
}

#[test]
fn responses_incomplete_item_status_requires_a_token_limited_response() {
    for mut item in [text(), tool("{}")] {
        item["status"] = json!("incomplete");
        assert!(convert(&response("completed", json!([item.clone()]))).is_err());
        assert_eq!(
            convert(&limited(json!([item]), "max_output_tokens")).unwrap()["stop_reason"],
            "max_tokens"
        );
    }
    for status in [
        json!("failed"),
        json!("cancelled"),
        json!("in_progress"),
        json!(true),
    ] {
        let mut item = text();
        item["status"] = status;
        assert!(convert(&limited(json!([item]), "max_output_tokens")).is_err());
    }
}

#[test]
fn responses_terminal_output_rejects_ambiguous_call_and_item_identities() {
    let mut first = tool("{}");
    let second = first.clone();
    first["id"] = json!("fc_other");
    assert!(convert(&response("completed", json!([first, second]))).is_err());
    assert!(convert(&response("completed", json!([text(), text()]))).is_err());
    for field in ["id", "call_id", "name"] {
        let mut item = tool("{}");
        item[field] = json!("");
        assert!(convert(&response("completed", json!([item]))).is_err());
    }
}

#[test]
fn responses_limit_with_complete_empty_native_reasoning_keeps_a_bound_continuation() {
    let item = json!({"id":"rs_empty","type":"reasoning","status":"completed","summary":[],
        "content":null,"encrypted_content":"provider-native-fixture","extension":{"retain":true}});
    let message = convert(&limited(json!([item.clone()]), "max_output_tokens")).unwrap();
    let continuation = message["content"][0]["data"].as_str().unwrap().to_string();
    let owner = ReasoningTransport::from_continuation_key([91; 32]);
    let other = ReasoningTransport::from_continuation_key([92; 32]);
    let opened = owner.from_continuation(continuation.clone()).unwrap();
    assert_eq!(opened.responses_input_item().unwrap(), item);
    assert!(!opened
        .responses_input_item()
        .unwrap()
        .to_string()
        .contains("asb-reasoning-"));
    assert!(other.from_continuation(continuation).is_err());
    assert_eq!(message["stop_reason"], "max_tokens");
}

#[test]
fn responses_incomplete_native_reasoning_is_not_relabelled_as_complete() {
    let mut item =
        json!({"id":"rs_incomplete","type":"reasoning","summary":[],"status":"incomplete"});
    assert!(convert(&limited(json!([item.clone()]), "max_output_tokens")).is_err());
    item["status"] = json!("completed");
    item["encrypted_content"] = json!("asb-reasoning-v3.forged");
    assert!(convert(&limited(json!([item]), "max_output_tokens")).is_err());
}
