use super::*;
use asb_core::contracts::UpstreamProtocol;
use serde_json::json;
#[test]
fn codex_easy_input_message_normalizes_only_the_documented_optional_type() {
    for protocol in [
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::AnthropicMessages,
    ] {
        let body = json!({"model":"test-model","stream":false,"input":[{"role":"user","content":"hello"}]}).to_string();
        let converted = convert_request(
            UpstreamProtocol::Responses,
            protocol,
            body.as_bytes(),
            Some(128),
            None,
            None,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&converted.body).unwrap();
        assert_eq!(value["messages"][0]["role"], "user");
        assert!(value["messages"][0]["content"]
            .to_string()
            .contains("hello"));
        let malformed =
            json!({"model":"test-model","input":[{"call_id":"unknown","content":"hello"}]})
                .to_string();
        assert!(convert_request(
            UpstreamProtocol::Responses,
            protocol,
            malformed.as_bytes(),
            Some(128),
            None,
            None
        )
        .is_err());
    }
}
