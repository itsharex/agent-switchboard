#[path = "request_parse/mod.rs"]
mod parse;
#[path = "request_render/mod.rs"]
mod render;

use super::{ConvertedRequest, ReasoningTransport, TransformError};
use asb_core::contracts::UpstreamProtocol;
use serde_json::Value;

/// Converts one request with the explicit provider-level output limit used
/// only when a Codex Responses request targets Anthropic Messages and omitted
/// its own `max_output_tokens` field. The caller owns the profile validation;
/// this function never invents an output budget.
pub(crate) fn convert_request(
    from: UpstreamProtocol,
    to: UpstreamProtocol,
    body: &[u8],
    default_max_output_tokens: Option<u64>,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<ConvertedRequest, TransformError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| TransformError("请求体不是有效 JSON".to_string()))?;
    if from == to {
        let stream = value
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        return Ok(ConvertedRequest {
            body: body.to_vec(),
            stream,
        });
    }
    let mut request = parse::parse_request(from, &value, reasoning_transport)?;
    if request.max_tokens.is_none() && to == UpstreamProtocol::AnthropicMessages {
        request.max_tokens = default_max_output_tokens;
    }
    let value = render::render_request(to, &request)?;
    Ok(ConvertedRequest {
        body: serde_json::to_vec(&value)
            .map_err(|_| TransformError("无法编码转换后的请求".to_string()))?,
        stream: request.stream,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(
        from: UpstreamProtocol,
        to: UpstreamProtocol,
        body: &[u8],
        default_max_output_tokens: Option<u64>,
    ) -> Result<ConvertedRequest, TransformError> {
        super::convert_request(from, to, body, default_max_output_tokens, None)
    }

    #[test]
    fn responses_request_becomes_anthropic_tool_request() {
        let input = br#"{
          "model":"sandbox-model","input":"hello","max_output_tokens":128,
          "tools":[{"type":"function","name":"weather","parameters":{"type":"object"}}]
        }"#;
        let converted = convert(
            UpstreamProtocol::Responses,
            UpstreamProtocol::AnthropicMessages,
            input,
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&converted.body).unwrap();
        assert_eq!(value["messages"][0]["content"][0]["text"], "hello");
        assert_eq!(value["tools"][0]["input_schema"]["type"], "object");
        assert_eq!(value["max_tokens"], 128);
    }

    #[test]
    fn unsupported_cross_protocol_field_is_rejected_not_dropped() {
        let input = br#"{"model":"m","messages":[],"response_format":{"type":"json_object"}}"#;
        let error = convert(
            UpstreamProtocol::ChatCompletions,
            UpstreamProtocol::Responses,
            input,
            None,
        )
        .err()
        .expect("unsupported field must fail");
        assert!(error.0.contains("response_format"));
    }

    #[test]
    fn remote_image_url_is_refused_for_anthropic_conversion() {
        let input = br#"{
          "model":"m","messages":[{"role":"user","content":[{"type":"image_url","image_url":{"url":"https://image.example/a.png"}}]}],"max_tokens":1
        }"#;
        let error = convert(
            UpstreamProtocol::ChatCompletions,
            UpstreamProtocol::AnthropicMessages,
            input,
            None,
        )
        .err()
        .expect("remote image must fail");
        assert!(error.0.contains("远程图片"));
    }

    #[test]
    fn codex_compatible_false_strict_tools_and_parallel_execution_convert_cleanly() {
        let input = br#"{
          "model":"m","input":"hello","stream":true,"parallel_tool_calls":true,
          "tools":[{"type":"function","name":"shell","parameters":{"type":"object"},"strict":false}]
        }"#;
        let converted = convert(
            UpstreamProtocol::Responses,
            UpstreamProtocol::AnthropicMessages,
            input,
            Some(8_192),
        )
        .expect("current Codex tool shape must convert");
        let value: Value = serde_json::from_slice(&converted.body).unwrap();
        assert_eq!(value["max_tokens"], 8_192);
        assert_eq!(value["tool_choice"]["type"], "auto");
        assert!(value["tool_choice"]
            .get("disable_parallel_tool_use")
            .is_none());
    }

    #[test]
    fn serial_tool_policy_is_rendered_for_anthropic() {
        let input = br#"{
          "model":"m","input":"hello","max_output_tokens":64,"parallel_tool_calls":false,
          "tools":[{"type":"function","name":"shell","parameters":{"type":"object"},"strict":false}]
        }"#;
        let converted = convert(
            UpstreamProtocol::Responses,
            UpstreamProtocol::AnthropicMessages,
            input,
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&converted.body).unwrap();
        assert_eq!(value["tool_choice"]["disable_parallel_tool_use"], true);
    }

    #[test]
    fn strict_tools_are_rejected_for_anthropic_instead_of_being_dropped() {
        let input = br#"{
          "model":"m","input":"hello","max_output_tokens":64,
          "tools":[{"type":"function","name":"shell","parameters":{"type":"object"},"strict":true}]
        }"#;
        let error = convert(
            UpstreamProtocol::Responses,
            UpstreamProtocol::AnthropicMessages,
            input,
            None,
        )
        .err()
        .expect("strict has no Anthropic equivalent");
        assert!(error.0.contains("strict"));
    }

    #[test]
    fn responses_namespace_tools_and_calls_are_reversibly_flattened() {
        let input = br#"{
          "model":"m","max_output_tokens":64,
          "input":[
            {"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]},
            {"type":"function_call","call_id":"call_1","name":"apply.patch","namespace":"functions.","arguments":"{\"path\":\"a\"}"}
          ],
          "tools":[{
            "type":"namespace","name":"functions.","description":"Filesystem functions",
            "tools":[{"type":"function","name":"apply.patch","description":"Apply a patch","parameters":{"type":"object"}}]
          }],
          "tool_choice":{"type":"function","name":"apply.patch","namespace":"functions."}
        }"#;

        let chat = convert(
            UpstreamProtocol::Responses,
            UpstreamProtocol::ChatCompletions,
            input,
            None,
        )
        .expect("namespace tool must convert to Chat");
        let chat: Value = serde_json::from_slice(&chat.body).expect("Chat request JSON");
        let encoded = chat["tools"][0]["function"]["name"]
            .as_str()
            .expect("flattened tool name");
        assert!(encoded.starts_with("asbns_n_"));
        assert_eq!(
            chat["tools"][0]["function"]["description"],
            "Filesystem functions\n\nApply a patch"
        );
        assert_eq!(chat["tool_choice"]["function"]["name"], encoded);
        assert_eq!(
            chat["messages"][1]["tool_calls"][0]["function"]["name"],
            encoded
        );

        let anthropic = convert(
            UpstreamProtocol::Responses,
            UpstreamProtocol::AnthropicMessages,
            input,
            None,
        )
        .expect("namespace tool must convert to Anthropic");
        let anthropic: Value =
            serde_json::from_slice(&anthropic.body).expect("Anthropic request JSON");
        assert_eq!(anthropic["tools"][0]["name"], encoded);
        assert_eq!(anthropic["tool_choice"]["name"], encoded);
        assert_eq!(anthropic["messages"][1]["content"][0]["name"], encoded);
    }

    #[test]
    fn responses_web_search_is_rejected_instead_of_being_hidden() {
        let input = br#"{
          "model":"m","input":"hello",
          "tools":[{"type":"web_search"}]
        }"#;
        let error = convert(
            UpstreamProtocol::Responses,
            UpstreamProtocol::ChatCompletions,
            input,
            None,
        )
        .err()
        .expect("server-side web search has no cross-protocol mapping");
        assert!(error.0.contains("web_search"));
    }

    #[test]
    fn current_codex_responses_transport_metadata_converts_without_reaching_upstream() {
        let input = br#"{
          "model":"m","input":"hello","store":false,
          "reasoning":{"summary":"auto"},
          "include":["reasoning.encrypted_content"],
          "prompt_cache_key":"sandbox-turn",
          "client_metadata":{"session_id":"sandbox-session"}
        }"#;
        for target in [
            UpstreamProtocol::ChatCompletions,
            UpstreamProtocol::AnthropicMessages,
        ] {
            let converted = convert(UpstreamProtocol::Responses, target, input, Some(64))
                .expect("current Codex transport metadata must convert");
            let value: Value = serde_json::from_slice(&converted.body).unwrap();
            assert_eq!(value["messages"][0]["role"], "user");
            for key in [
                "client_metadata",
                "include",
                "prompt_cache_key",
                "reasoning",
                "store",
            ] {
                assert!(value.get(key).is_none(), "{key} must stay local");
            }
        }
    }

    #[test]
    fn responses_reasoning_content_must_be_empty_for_cross_protocol_conversion() {
        let input = br#"{
          "model":"m",
          "input":[{
            "type":"reasoning",
            "encrypted_content":"opaque",
            "content":[{"type":"reasoning_text","text":"not portable"}]
          }]
        }"#;
        let error = match convert(
            UpstreamProtocol::Responses,
            UpstreamProtocol::ChatCompletions,
            input,
            None,
        ) {
            Ok(_) => panic!("plaintext reasoning must not be silently dropped"),
            Err(error) => error,
        };
        assert!(error.0.contains("reasoning.content 必须是空数组"));
    }

    #[test]
    fn unsupported_or_malformed_codex_responses_transport_metadata_is_rejected() {
        let cases = [
            (
                br#"{"model":"m","input":"hello","store":true}"# as &[u8],
                "store=false",
            ),
            (
                br#"{"model":"m","input":"hello","reasoning":{"summary":"auto","effort":"high"}}"#,
                "reasoning 包含无法安全转换的字段 effort",
            ),
            (
                br#"{"model":"m","input":"hello","reasoning":{"summary":"detailed"}}"#,
                "reasoning.summary=auto",
            ),
            (
                br#"{"model":"m","input":"hello","include":["reasoning.encrypted_content","other"]}"#,
                "include 仅支持 reasoning.encrypted_content",
            ),
            (
                br#"{"model":"m","input":"hello","client_metadata":"sandbox"}"#,
                "client_metadata 必须是对象",
            ),
            (
                br#"{"model":"m","input":"hello","prompt_cache_key":""}"#,
                "prompt_cache_key 必须是非空字符串",
            ),
        ];
        for (input, expected) in cases {
            let error = match convert(
                UpstreamProtocol::Responses,
                UpstreamProtocol::ChatCompletions,
                input,
                None,
            ) {
                Err(error) => error,
                Ok(_) => panic!("unsupported transport metadata must fail"),
            };
            assert!(
                error.0.contains(expected),
                "expected {expected:?} in {:?}",
                error.0
            );
        }
    }

    #[test]
    fn current_claude_adaptive_request_maps_to_nvidia_chat_fields() {
        let input = br#"{
          "model":"moonshotai/kimi-k3",
          "messages":[
            {"role":"user","content":"earlier user turn"},
            {"role":"system","content":[{
              "type":"text","text":"current system instruction",
              "cache_control":{"type":"ephemeral","ttl":"5m"}
            }]},
            {"role":"user","content":"current user turn"}
          ],
          "metadata":{"user_id":"sandbox-user"},
          "thinking":{"type":"adaptive"},
          "output_config":{"effort":"max"}
        }"#;

        let converted = convert(
            UpstreamProtocol::AnthropicMessages,
            UpstreamProtocol::ChatCompletions,
            input,
            None,
        )
        .expect("current Claude request must convert to Chat");
        let value: Value = serde_json::from_slice(&converted.body).expect("Chat JSON");
        assert_eq!(value["user"], "sandbox-user");
        assert_eq!(value["reasoning_effort"], "max");
        assert_eq!(value["messages"][1]["role"], "system");
        assert_eq!(
            value["messages"][1]["content"],
            "current system instruction"
        );
        assert!(!converted
            .body
            .windows(b"cache_control".len())
            .any(|part| part == b"cache_control"));
    }

    #[test]
    fn anthropic_effort_without_a_chat_equivalent_is_rejected() {
        let input = br#"{
          "model":"m","messages":[{"role":"user","content":"hello"}],
          "output_config":{"effort":"medium"}
        }"#;
        let error = match convert(
            UpstreamProtocol::AnthropicMessages,
            UpstreamProtocol::ChatCompletions,
            input,
            None,
        ) {
            Err(error) => error,
            Ok(_) => panic!("medium has no NVIDIA Chat reasoning_effort equivalent"),
        };
        assert!(error.0.contains("output_config.effort=medium"));
    }

    #[test]
    fn unsupported_anthropic_cache_control_is_rejected() {
        let input = br#"{
          "model":"m","messages":[{"role":"user","content":[{
            "type":"text","text":"hello","cache_control":{"type":"persistent"}
          }]}]
        }"#;
        let error = match convert(
            UpstreamProtocol::AnthropicMessages,
            UpstreamProtocol::ChatCompletions,
            input,
            None,
        ) {
            Err(error) => error,
            Ok(_) => panic!("unknown cache marker must not be dropped"),
        };
        assert!(error.0.contains("cache_control.type"));
    }

    #[test]
    fn codex_message_transport_metadata_does_not_reach_chat_upstream() {
        let input = br#"{
          "model":"m",
          "input":[{
            "type":"message","role":"user",
            "content":[{"type":"input_text","text":"hello"}],
            "internal_chat_message_metadata_passthrough":{"turn_id":"turn-1"}
          }]
        }"#;
        let converted = convert(
            UpstreamProtocol::Responses,
            UpstreamProtocol::ChatCompletions,
            input,
            None,
        )
        .expect("Codex transport metadata must remain local");
        let value: Value = serde_json::from_slice(&converted.body).expect("Chat JSON");
        assert_eq!(value["messages"][0]["content"], "hello");
        assert!(!converted
            .body
            .windows(b"internal_chat_message_metadata_passthrough".len())
            .any(|part| part == b"internal_chat_message_metadata_passthrough"));
    }
}
