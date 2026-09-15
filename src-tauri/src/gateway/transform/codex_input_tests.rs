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

#[test]
fn input_file_and_audio_convert_per_target_protocol() {
    // Chat: hosted id and data-URL references pass verbatim with the name;
    // an https-only reference has no Chat file form and is refused.
    let file_part = json!({"type":"input_file","file_data":"data:application/pdf;base64,AAA","filename":"spec.pdf"});
    let audio_part = json!({"type":"input_audio","input_audio":{"data":"QUFB","format":"wav"}});
    let chat_body = json!({"model":"m","stream":false,"input":[{"role":"user","content":[
        {"type":"input_text","text":"read"}, file_part, audio_part]}]})
    .to_string();
    let converted = convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        chat_body.as_bytes(),
        None,
        None,
        None,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&converted.body).unwrap();
    let content = &value["messages"][0]["content"];
    assert_eq!(content[1]["type"], "file");
    assert_eq!(
        content[1]["file"]["file_data"],
        "data:application/pdf;base64,AAA"
    );
    assert_eq!(content[1]["file"]["filename"], "spec.pdf");
    assert_eq!(content[2]["type"], "input_audio");
    assert_eq!(content[2]["input_audio"]["format"], "wav");

    let url_only = json!({"model":"m","stream":false,"input":[{"role":"user","content":[
        {"type":"input_file","file_url":"https://example.test/spec.pdf"}]}]})
    .to_string();
    assert!(convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        url_only.as_bytes(),
        None,
        None,
        None
    )
    .is_err());

    // Anthropic: https reference becomes a url document, data URL becomes a
    // base64 document titled by the file name, audio is refused.
    let anthropic_body = json!({"model":"m","stream":false,"max_output_tokens":64,"input":[{"role":"user","content":[
        {"type":"input_file","file_url":"https://example.test/spec.pdf","filename":"spec.pdf"}]}]}).to_string();
    let converted = convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::AnthropicMessages,
        anthropic_body.as_bytes(),
        Some(64),
        None,
        None,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&converted.body).unwrap();
    assert_eq!(value["messages"][0]["content"][0]["type"], "document");
    assert_eq!(value["messages"][0]["content"][0]["source"]["type"], "url");
    assert_eq!(value["messages"][0]["content"][0]["title"], "spec.pdf");

    let data_body = json!({"model":"m","stream":false,"max_output_tokens":64,"input":[{"role":"user","content":[
        {"type":"input_file","file_data":"data:application/pdf;base64,AAA"}]}]}).to_string();
    let converted = convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::AnthropicMessages,
        data_body.as_bytes(),
        Some(64),
        None,
        None,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&converted.body).unwrap();
    assert_eq!(
        value["messages"][0]["content"][0]["source"]["media_type"],
        "application/pdf"
    );
    assert_eq!(value["messages"][0]["content"][0]["source"]["data"], "AAA");

    let audio_anthropic = json!({"model":"m","stream":false,"max_output_tokens":64,"input":[{"role":"user","content":[
        audio_part]}]}).to_string();
    let error = convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::AnthropicMessages,
        audio_anthropic.as_bytes(),
        Some(64),
        None,
        None,
    )
    .err()
    .expect("audio must fail on Anthropic");
    assert!(error.0.contains("音频"), "{}", error.0);

    // Structurally invalid file inputs fail loudly instead of being dropped.
    let no_reference = json!({"model":"m","stream":false,"input":[{"role":"user","content":[
        {"type":"input_file","filename":"only.pdf"}]}]})
    .to_string();
    assert!(convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        no_reference.as_bytes(),
        None,
        None,
        None
    )
    .is_err());
    let broken_audio = json!({"model":"m","stream":false,"input":[{"role":"user","content":[
        {"type":"input_audio"}]}]})
    .to_string();
    assert!(convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        broken_audio.as_bytes(),
        None,
        None,
        None
    )
    .is_err());
}
