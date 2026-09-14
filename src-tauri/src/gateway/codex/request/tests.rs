use super::*;
use asb_core::contracts::CodexAnthropicCacheTtl;
#[test]
fn prompt_cache_auto_is_narrow_and_explicit_keys_win_over_real_sessions() {
    for (endpoint,enabled) in [("https://api.openai.com/v1",true),("https://api.kimi.com/coding/v1",true),
        ("https://api.kimi.com/v1",false),("https://api.openai.com.evil.test/v1",false),("https://relay.test/v1",false)] {
        let mut wire=json!({"model":"model","messages":[]});
        apply(UpstreamProtocol::ChatCompletions,endpoint,&Default::default(),&json!({"prompt_cache_key":"explicit"}),&mut wire,Some("real-session")).unwrap();
        assert_eq!(wire.get("prompt_cache_key"),enabled.then_some(&json!("explicit")),"{endpoint}");
    }
    let options=CodexRequestOptions {prompt_cache_routing:CodexPromptCacheRouting::Enabled,..Default::default()};
    let mut wire=json!({});apply(UpstreamProtocol::ChatCompletions,"https://relay.test",&options,&json!({}),&mut wire,Some("session")).unwrap();
    assert_eq!(wire["prompt_cache_key"],"session");
    let mut wire=json!({});apply(UpstreamProtocol::ChatCompletions,"https://relay.test",&options,&json!({}),&mut wire,None).unwrap();
    assert!(wire.get("prompt_cache_key").is_none());
}
#[test]
fn disabled_cache_routing_wins_and_provider_overrides_remain_explicit() {
    let mut wire=json!({"prompt_cache_key":"provider-override"});
    let options=CodexRequestOptions {prompt_cache_routing:CodexPromptCacheRouting::Disabled,..Default::default()};
    apply(UpstreamProtocol::ChatCompletions,"https://api.openai.com",&options,&json!({"prompt_cache_key":"client"}),&mut wire,Some("session")).unwrap();
    assert!(wire.get("prompt_cache_key").is_none());
    let mut wire=json!({"prompt_cache_key":"provider-override"});
    apply(UpstreamProtocol::ChatCompletions,"https://api.openai.com",&Default::default(),&json!({"prompt_cache_key":"client"}),&mut wire,Some("session")).unwrap();
    assert_eq!(wire["prompt_cache_key"],"provider-override");
}
#[test]
fn moonshot_schema_rewrite_preserves_literal_data_and_existing_constraints() {
    let schema=json!({"type":"object","properties":{"text":{"$ref":"#/$defs/label","type":"string","allOf":[{"minLength":2}],
        "default":{"$ref":"this-is-data","type":"literal"}}},"$defs":{"label":{"$ref":"#/definitions/base","description":"keep"}},
        "examples":[{"$ref":"also-data","type":"literal"}]});
    let source=json!({"model":"model","tools":[{"type":"function","function":{"name":"tool","parameters":schema}}]});
    let mut wire=source.clone();
    assert!(apply(UpstreamProtocol::ChatCompletions,"https://api.moonshot.cn/v1",&Default::default(),&json!({}),&mut wire,None).unwrap().0);
    let result=&wire["tools"][0]["function"]["parameters"];
    assert_eq!(result["properties"]["text"]["allOf"],json!([{"$ref":"#/$defs/label"},{"minLength":2}]));
    assert_eq!(result["properties"]["text"]["default"],schema["properties"]["text"]["default"]);
    assert_eq!(result["examples"],schema["examples"]);
    assert!(!apply(UpstreamProtocol::ChatCompletions,"https://api.moonshot.cn/v1",&Default::default(),&json!({}),&mut wire,None).unwrap().0);
    let mut other=source.clone();apply(UpstreamProtocol::ChatCompletions,"https://moonshot.cn.evil.test/v1",&Default::default(),&json!({}),&mut other,None).unwrap();
    assert_eq!(other,source);
}
#[test]
fn anthropic_compatibility_is_opt_in_and_one_m_is_request_specific() {
    let mut ordinary=json!({"model":"claude[1m]","system":"Original Codex instructions","messages":[{"role":"user","content":"hello"}]});
    let (_,beta)=apply(UpstreamProtocol::AnthropicMessages,"https://relay.test",&Default::default(),&json!({}),&mut ordinary,None).unwrap();
    assert_eq!(ordinary["model"],"claude");assert_eq!(beta.as_deref(),Some("context-1m-2025-08-07"));
    assert_eq!(ordinary["system"],"Original Codex instructions");assert!(!ordinary.to_string().contains("cache_control"));
    let options=CodexRequestOptions {anthropic_cache_ttl:Some(CodexAnthropicCacheTtl::OneHour),emulate_claude_code:true,..Default::default()};
    let (_,beta)=apply(UpstreamProtocol::AnthropicMessages,"https://relay.test",&options,&json!({}),&mut ordinary,None).unwrap();
    assert_eq!(beta.as_deref(),Some("claude-code-20250219"));
    assert_eq!(ordinary["system"][1]["text"],"Original Codex instructions");
    assert_eq!(ordinary["system"][1]["cache_control"]["ttl"],"1h");
    let after=ordinary.clone();apply(UpstreamProtocol::AnthropicMessages,"https://relay.test",&options,&json!({}),&mut ordinary,None).unwrap();
    assert_eq!(ordinary,after);
}
#[test]
fn anthropic_cache_respects_four_breakpoints_and_manual_ttl_order() {
    let options=CodexRequestOptions {anthropic_cache_ttl:Some(CodexAnthropicCacheTtl::FiveMinutes),..Default::default()};
    let mut body=json!({"model":"claude","system":"system","tools":[{"name":"tool","input_schema":{"type":"object"}}],
        "messages":[{"role":"user","content":"old"},{"role":"assistant","content":"reply"},{"role":"user","content":"new"}]});
    apply(UpstreamProtocol::AnthropicMessages,"https://relay.test",&options,&json!({}),&mut body,None).unwrap();
    assert_eq!(body.to_string().matches("cache_control").count(),4);
    let mut manual=json!({"model":"claude","system":[{"type":"text","text":"manual","cache_control":{"type":"ephemeral","ttl":"5m"}}],
        "messages":[{"role":"user","content":"unchanged"}]});let before=manual.clone();
    let options=CodexRequestOptions {anthropic_cache_ttl:Some(CodexAnthropicCacheTtl::OneHour),..Default::default()};
    apply(UpstreamProtocol::AnthropicMessages,"https://relay.test",&options,&json!({}),&mut manual,None).unwrap();
    assert_eq!(manual,before);
}
