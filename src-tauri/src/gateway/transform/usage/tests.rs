use super::*;

#[test]
fn anthropic_cache_counters_normalize_to_total_input_and_roundtrip() {
    let original = json!({"input_tokens":20,"output_tokens":7,"cache_read_input_tokens":30,"cache_creation_input_tokens":10});
    let usage = parse(UpstreamProtocol::AnthropicMessages, Some(&original)).unwrap();
    assert_eq!(usage.input_tokens, Some(60));
    assert_eq!(usage.total_tokens, Some(67));
    assert_eq!(anthropic_json(&usage), original);
    assert_eq!(responses_json(&usage)["input_tokens"], 60);
    assert_eq!(
        responses_json(&usage)["input_tokens_details"]["cached_tokens"],
        30
    );
    assert_eq!(chat_json(&usage)["prompt_tokens"], 60);
}

#[test]
fn openai_total_input_does_not_double_count_anthropic_cache_reads() {
    for (protocol, original) in [
        (
            UpstreamProtocol::ChatCompletions,
            json!({"prompt_tokens":100,"completion_tokens":7,"prompt_tokens_details":{"cached_tokens":40}}),
        ),
        (
            UpstreamProtocol::Responses,
            json!({"input_tokens":100,"output_tokens":7,"input_tokens_details":{"cached_tokens":40}}),
        ),
    ] {
        let usage = parse(protocol, Some(&original)).unwrap();
        let anthropic = anthropic_json(&usage);
        assert_eq!(anthropic["input_tokens"], 60);
        assert_eq!(anthropic["cache_read_input_tokens"], 40);
        let roundtrip = parse(UpstreamProtocol::AnthropicMessages, Some(&anthropic)).unwrap();
        assert_eq!(roundtrip.input_tokens, Some(100));
    }
}

#[test]
fn late_stream_usage_preserves_reported_input_without_fabricating_it() {
    let unknown = Usage::default();
    assert_eq!(
        anthropic_message_delta_json(&unknown),
        json!({"output_tokens":0})
    );
    let known = parse(
        UpstreamProtocol::ChatCompletions,
        Some(&json!({"prompt_tokens":11,"completion_tokens":3,"prompt_cache_hit_tokens":4})),
    )
    .unwrap();
    assert_eq!(
        anthropic_message_delta_json(&known),
        json!({"input_tokens":7,"output_tokens":3,"cache_read_input_tokens":4})
    );
}

#[test]
fn partial_stream_usage_keeps_input_and_recomputes_stale_totals() {
    let mut usage = parse(UpstreamProtocol::ChatCompletions, Some(&json!({"prompt_tokens":11,"completion_tokens":1,"total_tokens":12,"prompt_cache_hit_tokens":4}))).unwrap();
    let final_usage = parse(
        UpstreamProtocol::ChatCompletions,
        Some(&json!({"completion_tokens":7})),
    )
    .unwrap();
    usage.merge_from(&final_usage);
    assert_eq!(chat_json(&usage)["total_tokens"], 18);
    assert_eq!(
        anthropic_message_delta_json(&usage),
        json!({"input_tokens":7,"output_tokens":7,"cache_read_input_tokens":4})
    );
}

#[test]
fn missing_or_null_usage_is_unavailable_and_large_counters_do_not_overflow() {
    for protocol in [
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::AnthropicMessages,
    ] {
        assert!(parse(protocol, Some(&Value::Null))
            .unwrap()
            .input_tokens
            .is_none());
    }
    let usage = parse(
        UpstreamProtocol::AnthropicMessages,
        Some(&json!({"input_tokens":u64::MAX,"cache_read_input_tokens":1,"output_tokens":1})),
    )
    .unwrap();
    assert_eq!(usage.total_tokens, Some(u64::MAX));
    assert_eq!(
        responses_json(&Usage {
            input_tokens: Some(u64::MAX),
            output_tokens: Some(1),
            ..Usage::default()
        })["total_tokens"],
        u64::MAX
    );
}
