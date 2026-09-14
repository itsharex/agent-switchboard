use super::*;

fn assert_effort(input: &Value, expected: Option<&str>) {
    for target in TARGETS {
        let output = convert(input, target).expect("supported Claude reasoning directive");
        assert_eq!(output_effort(&output, target), expected, "{input}");
        assert!(output.get("thinking").is_none());
        assert!(output.get("output_config").is_none());
    }
}

#[test]
fn enabled_budgets_use_the_upstream_thresholds_for_both_targets() {
    for (thinking, expected) in [
        (json!({ "type": "enabled", "budget_tokens": 0 }), "low"),
        (json!({ "type": "enabled", "budget_tokens": 1024 }), "low"),
        (json!({ "type": "enabled", "budget_tokens": 3999 }), "low"),
        (
            json!({ "type": "enabled", "budget_tokens": 4000 }),
            "medium",
        ),
        (
            json!({ "type": "enabled", "budget_tokens": 15999 }),
            "medium",
        ),
        (json!({ "type": "enabled", "budget_tokens": 16000 }), "high"),
        (json!({ "type": "enabled", "budget_tokens": 32000 }), "high"),
        (json!({ "type": "enabled" }), "high"),
    ] {
        let mut input = request("gpt-5.4");
        input["thinking"] = thinking;
        assert_effort(&input, Some(expected));
    }
}

#[test]
fn adaptive_disabled_and_absent_thinking_have_distinct_effort_behavior() {
    let mut input = request("gpt-5.4");
    assert_effort(&input, None);
    input["thinking"] = json!({ "type": "adaptive" });
    assert_effort(&input, Some("xhigh"));
    input["thinking"] = json!({ "type": "disabled" });
    assert_effort(&input, None);
}

#[test]
fn explicit_effort_takes_priority_over_every_thinking_mode() {
    for (effort, expected) in [
        ("low", "low"),
        ("medium", "medium"),
        ("high", "high"),
        ("max", "xhigh"),
    ] {
        let mut input = request("gpt-5.4");
        input["output_config"] = json!({ "effort": effort });
        assert_effort(&input, Some(expected));
        for thinking in [
            json!({ "type": "enabled", "budget_tokens": 32000 }),
            json!({ "type": "adaptive" }),
            json!({ "type": "disabled" }),
        ] {
            input["thinking"] = thinking;
            assert_effort(&input, Some(expected));
        }
    }
}

#[test]
fn empty_output_config_keeps_the_thinking_fallback() {
    let mut input = request("gpt-5.4");
    input["output_config"] = json!({});
    input["thinking"] = json!({ "type": "enabled", "budget_tokens": 8000 });
    assert_effort(&input, Some("medium"));
}

#[test]
fn emitted_chat_maximum_is_accepted_when_converted_to_responses() {
    let mut input = request("gpt-5.4");
    input["output_config"] = json!({ "effort": "max" });
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
    assert_eq!(output["reasoning"]["effort"], "xhigh");
}

#[test]
fn effort_is_only_injected_into_supported_model_families() {
    for model in [
        "o3-mini",
        "gpt-5.4",
        "GPT-5.4",
        "openai/gpt-5.4",
        "grok-4.5",
        "grok-build-0.1",
    ] {
        let mut input = request(model);
        input["output_config"] = json!({ "effort": "max" });
        assert_effort(&input, Some("xhigh"));
    }
    for model in ["gpt-4o", "claude-sonnet-4-6", "unknown-model"] {
        let mut input = request(model);
        input["thinking"] = json!({ "type": "adaptive" });
        input["output_config"] = json!({ "effort": "max" });
        assert_effort(&input, None);
    }
}

#[test]
fn the_existing_kimi_chat_dialect_does_not_receive_openai_xhigh() {
    for (effort, expected) in [
        ("low", "low"),
        ("medium", "high"),
        ("high", "high"),
        ("max", "max"),
    ] {
        let mut input = request("moonshotai/kimi-k3");
        input["output_config"] = json!({ "effort": effort });
        let output = convert(&input, UpstreamProtocol::ChatCompletions).unwrap();
        assert_eq!(output["reasoning_effort"], expected);
    }
    let mut input = request("moonshotai/kimi-k3");
    input["thinking"] = json!({ "type": "adaptive" });
    let output = convert(&input, UpstreamProtocol::ChatCompletions).unwrap();
    assert_eq!(output["reasoning_effort"], "max");
}

#[test]
fn malformed_thinking_and_effort_are_not_silently_treated_as_absent() {
    let cases = [
        ("thinking", Value::Null, "thinking"),
        ("thinking", json!("enabled"), "thinking"),
        ("thinking", json!({}), "thinking.type"),
        ("thinking", json!({ "type": "unknown" }), "thinking.type"),
        (
            "thinking",
            json!({ "type": "enabled", "budget_tokens": -1 }),
            "budget_tokens",
        ),
        (
            "thinking",
            json!({ "type": "enabled", "budget_tokens": 1.5 }),
            "budget_tokens",
        ),
        (
            "thinking",
            json!({ "type": "enabled", "budget_tokens": "8000" }),
            "budget_tokens",
        ),
        (
            "thinking",
            json!({ "type": "enabled", "budget_tokens": null }),
            "budget_tokens",
        ),
        (
            "thinking",
            json!({ "type": "disabled", "budget_tokens": 8000 }),
            "budget_tokens",
        ),
        (
            "thinking",
            json!({ "type": "adaptive", "budget_tokens": 8000 }),
            "budget_tokens",
        ),
        ("output_config", Value::Null, "output_config"),
        ("output_config", json!({ "effort": null }), "effort"),
        ("output_config", json!({ "effort": 3 }), "effort"),
        ("output_config", json!({ "effort": "turbo" }), "effort"),
        ("output_config", json!({ "effort": "xhigh" }), "effort"),
        (
            "output_config",
            json!({ "effort": "high", "unknown": true }),
            "unknown",
        ),
    ];
    for (field, value, expected) in cases {
        let mut input = request("gpt-5.4");
        input[field] = value;
        for target in TARGETS {
            let error = convert(&input, target).expect_err("invalid reasoning field");
            assert!(error.0.contains(expected), "{input}: {}", error.0);
        }
    }
}

#[test]
fn a_tool_call_without_reasoning_never_gets_a_fabricated_trace() {
    let mut input = request("gpt-5.4");
    input["thinking"] = json!({ "type": "adaptive" });
    input["messages"].as_array_mut().unwrap().push(json!({
        "role": "assistant", "content": [tool_call("capture-1")],
    }));
    for target in TARGETS {
        let output = convert(&input, target).unwrap();
        let text = serde_json::to_string(&output).unwrap();
        assert!(!text.contains("reasoning_content"));
        assert!(!text.contains("redacted thinking"));
        assert!(!text.contains("encrypted_content"));
    }
}

#[test]
fn native_readable_thinking_can_reach_chat_but_opaque_thinking_is_not_faked() {
    let transport = ReasoningTransport::from_continuation_key([41; 32]);
    let mut input = request("moonshotai/kimi-k3");
    input["messages"].as_array_mut().unwrap().push(json!({
        "role": "assistant", "content": [
            { "type": "thinking", "thinking": "inspect the capture", "signature": "upstream-signature" },
            tool_call("capture-1"),
        ],
    }));
    let output =
        convert_with_transport(&input, UpstreamProtocol::ChatCompletions, Some(&transport))
            .unwrap();
    assert_eq!(
        output["messages"][1]["reasoning_content"],
        "inspect the capture"
    );
    input["messages"][1]["content"][0] =
        json!({ "type": "redacted_thinking", "data": "opaque-upstream" });
    let error = convert_with_transport(&input, UpstreamProtocol::ChatCompletions, Some(&transport))
        .unwrap_err();
    assert!(error.0.contains("Chat Completions"));
}
