use super::*;
use serde_json::json;

#[test]
fn minimal_omits_extensions_and_preserves_selected_tool_and_generation_semantics() {
    let original = json!({
        "model":"custom/model", "input":[{"type":"function_call_output","call_id":"call-1","output":"done"}],
        "instructions":"Do the requested work", "stream":true, "max_output_tokens":512,
        "tools":[{"type":"function","name":"write","parameters":{"type":"object"},"strict":true}],
        "tool_choice":{"type":"function","name":"write"}, "parallel_tool_calls":false,
        "temperature":0.2, "top_p":0.8,
        "reasoning":{"effort":"high"}, "service_tier":"priority", "store":false,
        "include":["reasoning.encrypted_content"], "metadata":{"user":"test"},
        "client_metadata":{"session_id":"sandbox"}, "prompt_cache_key":"sandbox"
    });
    let converted = apply(
        ConvertedRequest {
            body: original.to_string().into_bytes(),
            stream: true,
        },
        Some(ResponsesOptions {
            request_mode: ResponsesRequestMode::Minimal,
        }),
    )
    .unwrap();
    let result: Value = serde_json::from_slice(&converted.body).unwrap();
    for name in KEPT_FIELDS {
        assert_eq!(result[*name], original[*name], "{name}");
    }
    for name in OMITTED_FIELDS {
        assert!(result.get(*name).is_none(), "{name}");
    }
}

#[test]
fn minimal_refuses_unknown_parameters_and_unreplayed_context() {
    for (field, value) in [
        ("previous_response_id", json!("resp-1")),
        ("background", json!(true)),
        ("store", json!(true)),
    ] {
        let mut request = json!({"model":"m","input":"hello"});
        request[field] = value;
        let error = filter_fields(request.as_object_mut().unwrap()).unwrap_err();
        assert!(error.0.contains(field));
    }
}

#[test]
fn minimal_refuses_opaque_reasoning_and_remote_input_references() {
    for kind in ["reasoning", "item_reference"] {
        let mut request =
            json!({"model":"m", "input":[{"type":kind,"encrypted_content":"opaque"}]});
        let error = filter_fields(request.as_object_mut().unwrap()).unwrap_err();
        assert!(error.0.contains("完整可见上下文"));
    }
}

#[test]
fn standard_request_retains_every_declared_responses_field() {
    let body =
        br#"{"model":"m","input":"hello","reasoning":{"effort":"high"},"service_tier":"priority"}"#
            .to_vec();
    let converted = apply(
        ConvertedRequest {
            body: body.clone(),
            stream: false,
        },
        Some(ResponsesOptions {
            request_mode: ResponsesRequestMode::Standard,
        }),
    )
    .unwrap();
    assert_eq!(converted.body, body);
}
