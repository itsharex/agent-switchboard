//! Reasoning replay and explicit Chat dialect conversion coverage.

use super::*;
use asb_core::contracts::{
    CodexChatEffortMode as EffortMode, CodexChatEffortParameter as EffortParameter,
    CodexChatReasoning, CodexChatThinkingParameter as ThinkingParameter,
};
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

const THINKING_PARAMETERS: [ThinkingParameter; 4] = [
    ThinkingParameter::None,
    ThinkingParameter::Thinking,
    ThinkingParameter::EnableThinking,
    ThinkingParameter::ReasoningSplit,
];
const EFFORT_PARAMETERS: [EffortParameter; 3] = [
    EffortParameter::None,
    EffortParameter::ReasoningEffort,
    EffortParameter::ReasoningObject,
];
const EFFORT_MODES: [EffortMode; 4] = [
    EffortMode::Passthrough,
    EffortMode::LowHigh,
    EffortMode::DeepSeek,
    EffortMode::OpenRouter,
];
const DISABLED_EFFORTS: [&str; 6] = ["none", "off", "disabled", "NONE", "Off", "DISABLED"];

fn chat_dialect(
    thinking: ThinkingParameter,
    effort: EffortParameter,
    mode: EffortMode,
) -> CodexChatReasoning {
    CodexChatReasoning::Configured {
        thinking_parameter: thinking,
        effort_parameter: effort,
        effort_mode: mode,
    }
}

fn chat_request(
    reasoning: Option<Value>,
    configuration: Option<&CodexChatReasoning>,
) -> Result<Value, TransformError> {
    let mut request = json!({"model":"sandbox-model", "input":"hello", "stream":true});
    if let Some(reasoning) = reasoning {
        request["reasoning"] = reasoning;
    }
    let converted = super::convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        &serde_json::to_vec(&request).expect("encode request"),
        None,
        None,
        configuration,
    )?;
    assert!(
        converted.stream,
        "reasoning must not change the stream mode"
    );
    Ok(serde_json::from_slice(&converted.body).expect("converted Chat JSON"))
}

fn control_fields(request: &Value) -> Value {
    Value::Object(
        [
            "thinking",
            "enable_thinking",
            "reasoning_split",
            "reasoning_effort",
            "reasoning",
        ]
        .into_iter()
        .filter_map(|key| request.get(key).map(|value| (key.into(), value.clone())))
        .collect(),
    )
}

fn expected_controls(
    thinking: ThinkingParameter,
    enabled: bool,
    effort: EffortParameter,
    level: Option<&str>,
) -> Value {
    let mut expected = match thinking {
        ThinkingParameter::None => json!({}),
        ThinkingParameter::Thinking => {
            json!({"thinking":{"type": if enabled { "enabled" } else { "disabled" }}})
        }
        ThinkingParameter::EnableThinking => json!({"enable_thinking":enabled}),
        ThinkingParameter::ReasoningSplit => json!({"reasoning_split":enabled}),
    };
    if let Some(level) = level {
        match effort {
            EffortParameter::None => {}
            EffortParameter::ReasoningEffort => expected["reasoning_effort"] = json!(level),
            EffortParameter::ReasoningObject => expected["reasoning"] = json!({"effort":level}),
        }
    }
    expected
}

#[test]
fn chat_reasoning_disabling_uses_only_declared_controls() {
    for mode in EFFORT_MODES {
        for thinking in THINKING_PARAMETERS {
            for effort in EFFORT_PARAMETERS {
                let configuration = chat_dialect(thinking, effort, mode);
                let unsupported = thinking == ThinkingParameter::None
                    && (effort == EffortParameter::None
                        || (effort == EffortParameter::ReasoningEffort
                            && mode != EffortMode::Passthrough));
                for alias in DISABLED_EFFORTS {
                    let result = chat_request(Some(json!({"effort":alias})), Some(&configuration));
                    if unsupported {
                        assert!(result.is_err(), "must refuse {configuration:?}, {alias}");
                        continue;
                    }
                    let level = match (thinking, effort) {
                        (_, EffortParameter::ReasoningObject)
                        | (ThinkingParameter::None, EffortParameter::ReasoningEffort) => {
                            Some("none")
                        }
                        _ => None,
                    };
                    let request =
                        result.unwrap_or_else(|error| panic!("{configuration:?}: {error}"));
                    assert_eq!(
                        control_fields(&request),
                        expected_controls(thinking, false, effort, level),
                        "{configuration:?}, {alias}"
                    );
                }
            }
        }
    }
}

#[test]
fn chat_reasoning_enabling_uses_only_declared_controls() {
    for mode in EFFORT_MODES {
        for thinking in THINKING_PARAMETERS {
            for effort in EFFORT_PARAMETERS {
                let configuration = chat_dialect(thinking, effort, mode);
                let result = chat_request(Some(json!({"effort":"HIGH"})), Some(&configuration));
                let request = result.unwrap_or_else(|error| panic!("{configuration:?}: {error}"));
                assert_eq!(
                    control_fields(&request),
                    expected_controls(thinking, true, effort, Some("high")),
                    "{configuration:?}"
                );
            }
        }
    }
}

#[test]
fn chat_reasoning_effort_modes_keep_their_declared_mappings() {
    let levels = ["minimal", "low", "medium", "high", "xhigh", "max", "ultra"];
    let cases = [
        (EffortMode::Passthrough, levels),
        (
            EffortMode::LowHigh,
            ["low", "low", "high", "high", "high", "high", "high"],
        ),
        (
            EffortMode::DeepSeek,
            ["high", "high", "high", "high", "max", "max", "max"],
        ),
        (
            EffortMode::OpenRouter,
            [
                "minimal", "low", "medium", "high", "xhigh", "xhigh", "xhigh",
            ],
        ),
    ];
    for (mode, expected) in cases {
        for effort in [
            EffortParameter::ReasoningEffort,
            EffortParameter::ReasoningObject,
        ] {
            let configuration = chat_dialect(ThinkingParameter::None, effort, mode);
            for (level, mapped) in levels.into_iter().zip(expected) {
                let request = chat_request(Some(json!({"effort":level})), Some(&configuration))
                    .unwrap_or_else(|error| panic!("{configuration:?}, {level}: {error}"));
                assert_eq!(
                    control_fields(&request),
                    expected_controls(ThinkingParameter::None, true, effort, Some(mapped)),
                    "{configuration:?}, {level}"
                );
            }
        }
    }
}

#[test]
fn chat_reasoning_invalid_efforts_are_not_coerced_or_dropped() {
    let invalid = [
        json!("turbo"),
        json!(""),
        json!("high "),
        json!(false),
        json!(7),
        Value::Null,
    ];
    for mode in EFFORT_MODES {
        for thinking in THINKING_PARAMETERS {
            for effort in EFFORT_PARAMETERS {
                let configuration = chat_dialect(thinking, effort, mode);
                for value in &invalid {
                    assert!(
                        chat_request(Some(json!({"effort":value})), Some(&configuration)).is_err(),
                        "must refuse {configuration:?}, {value}"
                    );
                }
            }
        }
    }
}

#[test]
fn chat_reasoning_without_effort_does_not_invent_controls() {
    for mode in EFFORT_MODES {
        for thinking in THINKING_PARAMETERS {
            for effort in EFFORT_PARAMETERS {
                let configuration = chat_dialect(thinking, effort, mode);
                for reasoning in [None, Some(json!({"summary":"auto"}))] {
                    let request = chat_request(reasoning, Some(&configuration)).expect("no effort");
                    assert_eq!(control_fields(&request), json!({}), "{configuration:?}");
                }
            }
        }
    }
}

#[test]
fn chat_reasoning_explicit_effort_requires_a_configured_dialect() {
    for configuration in [None, Some(CodexChatReasoning::Unsupported)] {
        for effort in ["high", "none", "off", "disabled"] {
            assert!(
                chat_request(Some(json!({"effort":effort})), configuration.as_ref()).is_err(),
                "must refuse {configuration:?}, {effort}"
            );
        }
        let request = chat_request(None, configuration.as_ref()).expect("no effort");
        assert_eq!(control_fields(&request), json!({}));
    }
}
