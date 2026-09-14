use asb_core::contracts::CodexRequestOptions;
use serde_json::{json,Value};
const IDENTITY:&str="You are Claude Code, Anthropic's official CLI for Claude.";
pub(super) fn rewrite(body:&mut Value, options:&CodexRequestOptions) -> (bool,Option<String>) {
    let mut changed=false; let mut beta=Vec::new();
    if let Some(model)=body.get("model").and_then(Value::as_str) {
        if model.to_ascii_lowercase().ends_with("[1m]") {
            let model=model[..model.len()-4].trim_end().to_string();
            body["model"]=json!(model); changed=true; beta.push("context-1m-2025-08-07");
        }
    }
    if options.emulate_claude_code {
        changed |= prepend_identity(body); beta.push("claude-code-20250219");
    }
    if let Some(ttl)=options.anthropic_cache_ttl { changed |= cache(body,ttl.as_str()); }
    (changed,(!beta.is_empty()).then(||beta.join(",")))
}
fn prepend_identity(body:&mut Value) -> bool {
    if body.get("system").and_then(Value::as_array).is_some_and(|blocks|blocks.first()
        .is_some_and(|block|block["text"]==IDENTITY)) { return false; }
    let system=body.as_object_mut().unwrap().remove("system");
    let mut blocks=vec![json!({"type":"text","text":IDENTITY})];
    match system { Some(Value::Array(existing))=>blocks.extend(existing),
        Some(Value::String(text)) if !text.is_empty()=>blocks.push(json!({"type":"text","text":text})),
        _=>{} }
    body["system"]=json!(blocks); true
}
fn explicit_cache(body:&Value) -> bool {
    let marked=|block:&Value|block.get("cache_control").is_some();
    body.get("system").and_then(Value::as_array).into_iter().flatten().any(marked)
        || body.get("tools").and_then(Value::as_array).into_iter().flatten().any(marked)
        || body.get("messages").and_then(Value::as_array).into_iter().flatten()
            .flat_map(|message|message.get("content").and_then(Value::as_array).into_iter().flatten()).any(marked)
}
/// Existing manual cache controls own the whole breakpoint plan, including TTL ordering.
fn cache(body:&mut Value,ttl:&str) -> bool {
    if explicit_cache(body) { return false; }
    if let Some(text)=body.get("system").and_then(Value::as_str) {
        if !text.is_empty() { body["system"]=json!([{"type":"text","text":text}]); }
    }
    let mut marked=0;
    if let Some(block)=body.get_mut("tools").and_then(Value::as_array_mut).and_then(|tools|tools.last_mut()) {
        mark(block,ttl); marked+=1;
    }
    if let Some(block)=body.get_mut("system").and_then(Value::as_array_mut).and_then(|blocks|blocks.last_mut()) {
        mark(block,ttl); marked+=1;
    }
    if let Some(messages)=body.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages.iter_mut().rev().take(2) {
            if marked>=4 { break; }
            if let Some(text)=message.get("content").and_then(Value::as_str) {
                if !text.is_empty() { message["content"]=json!([{"type":"text","text":text}]); }
            }
            if let Some(block)=message.get_mut("content").and_then(Value::as_array_mut)
                .and_then(|blocks|blocks.iter_mut().rev().find(|block|matches!(block["type"].as_str(),Some("text"|"image"|"tool_use"|"tool_result")))) {
                mark(block,ttl); marked+=1;
            }
        }
    }
    marked>0
}
fn mark(block:&mut Value,ttl:&str) { if let Some(block)=block.as_object_mut() { block.insert("cache_control".into(),json!({"type":"ephemeral","ttl":ttl})); } }
