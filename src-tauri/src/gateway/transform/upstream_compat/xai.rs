//! xAI native Responses compatibility for the Codex gateway.
//!
//! Codex sends `wire_api="responses"` bodies with OpenAI-backend-private
//! fields and tool carriers that xAI's strict `api.x.ai/v1/responses` parser
//! rejects with 400/422. The Chat/Anthropic conversions already drop these in
//! passing; the native passthrough forwards verbatim, so they are scrubbed
//! here. Deterministic and idempotent; every xAI-only rewrite stays in this
//! file behind the gate in the parent module.

use serde_json::{Map, Value};

/// Codex plugin-private fields removed recursively at any nesting depth.
const RECURSIVE_UNSUPPORTED_FIELDS: &[&str] = &["external_web_access"];

/// Top-level request fields xAI rejects regardless of model.
const TOP_LEVEL_UNSUPPORTED_FIELDS: &[&str] = &["prompt_cache_retention", "safety_identifier"];

/// Top-level sampling fields rejected specifically by grok-4.5.
const GROK_45_UNSUPPORTED_FIELDS: &[&str] = &[
    "presence_penalty",
    "presencePenalty",
    "frequency_penalty",
    "frequencyPenalty",
    "stop",
];

/// Tool `type` values xAI's Responses schema accepts; anything else is a
/// Codex/OpenAI private carrier the strict parser would reject.
const SUPPORTED_TOOL_TYPES: &[&str] = &[
    "function",
    "web_search",
    "x_search",
    "image_generation",
    "collections_search",
    "file_search",
    "code_execution",
    "code_interpreter",
    "mcp",
    "shell",
];

/// Strips xAI-unsupported fields and tools from a native Codex Responses
/// request body in place; runs after namespace flattening so lifted function
/// tools survive the type whitelist.
pub(super) fn sanitize(body: &mut Value) -> bool {
    if !body.is_object() {
        return false;
    }
    let mut changed = false;
    for field in TOP_LEVEL_UNSUPPORTED_FIELDS {
        changed |= remove_top_level(body, field);
    }
    if targets_grok_45(body) {
        for field in GROK_45_UNSUPPORTED_FIELDS {
            changed |= remove_top_level(body, field);
        }
    }
    for field in RECURSIVE_UNSUPPORTED_FIELDS {
        changed |= remove_recursive(body, field);
    }
    changed |= promote_additional_tools(body);
    changed |= strip_null_reasoning_content(body);
    changed |= filter_unsupported_tools(body);
    changed |= collapse_union_parameter_schemas(body);
    changed |= rewrite_agent_message(body);
    changed
}

/// xAI emits whole floats where a tool schema declared integers; Codex's
/// argument validation rejects them. Rewrites `12.0` to `12` inside
/// `function_call` arguments (parsed JSON strings) anywhere in a response.
pub(super) fn normalize_integer_arguments(value: &mut Value) -> bool {
    let mut changed = false;
    match value {
        Value::Array(items) => {
            for item in items {
                changed |= normalize_integer_arguments(item);
            }
        }
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("function_call") {
                if let Some(arguments) = object.get("arguments").and_then(Value::as_str) {
                    if let Ok(mut parsed) = serde_json::from_str::<Value>(arguments) {
                        if normalize_whole_floats(&mut parsed) {
                            if let Ok(encoded) = serde_json::to_string(&parsed) {
                                object.insert("arguments".to_string(), Value::String(encoded));
                                changed = true;
                            }
                        }
                    }
                }
            }
            for child in object.values_mut() {
                changed |= normalize_integer_arguments(child);
            }
        }
        _ => {}
    }
    changed
}

fn normalize_whole_floats(value: &mut Value) -> bool {
    let mut changed = false;
    match value {
        Value::Array(items) => {
            for item in items {
                changed |= normalize_whole_floats(item);
            }
        }
        Value::Object(object) => {
            for child in object.values_mut() {
                changed |= normalize_whole_floats(child);
            }
        }
        Value::Number(number) => {
            if let Some(float) = number.as_f64() {
                if float.is_finite()
                    && float.fract() == 0.0
                    && float.abs() <= i64::MAX as f64
                    && !number.is_i64()
                {
                    *number = serde_json::Number::from(float as i64);
                    changed = true;
                }
            }
        }
        _ => {}
    }
    changed
}

fn targets_grok_45(body: &Value) -> bool {
    let Some(model) = body.get("model").and_then(Value::as_str) else {
        return false;
    };
    let model = model.trim();
    let model = model.rsplit('/').next().unwrap_or(model).trim();
    model.eq_ignore_ascii_case("grok-4.5")
}

fn remove_top_level(body: &mut Value, field: &str) -> bool {
    body.as_object_mut()
        .and_then(|object| object.remove(field))
        .is_some()
}

fn remove_recursive(value: &mut Value, field: &str) -> bool {
    match value {
        Value::Object(map) => {
            let mut changed = map.remove(field).is_some();
            for child in map.values_mut() {
                changed |= remove_recursive(child, field);
            }
            changed
        }
        Value::Array(items) => {
            let mut changed = false;
            for child in items.iter_mut() {
                changed |= remove_recursive(child, field);
            }
            changed
        }
        _ => false,
    }
}

fn is_additional_tools_item(item: &Value) -> bool {
    item.get("type").and_then(Value::as_str).map(str::trim) == Some("additional_tools")
}

/// Lifts the Responses-Lite private `additional_tools` input carrier into
/// top-level `tools`, de-duplicated, and drops the carrier items.
fn promote_additional_tools(body: &mut Value) -> bool {
    let input_items: Vec<Value> = match body.get("input").and_then(Value::as_array) {
        Some(items) if items.iter().any(is_additional_tools_item) => items.clone(),
        _ => return false,
    };
    let mut merged: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        for tool in tools {
            seen.insert(tool_dedup_key(tool));
            merged.push(tool.clone());
        }
    }
    let mut filtered_input = Vec::with_capacity(input_items.len());
    let mut promoted = false;
    for item in input_items {
        if is_additional_tools_item(&item) {
            if let Some(carrier_tools) = item.get("tools").and_then(Value::as_array) {
                for tool in carrier_tools {
                    if seen.insert(tool_dedup_key(tool)) {
                        merged.push(tool.clone());
                        promoted = true;
                    }
                }
            }
            continue;
        }
        filtered_input.push(item);
    }
    if let Some(object) = body.as_object_mut() {
        object.insert("input".to_string(), Value::Array(filtered_input));
        if promoted {
            object.insert("tools".to_string(), Value::Array(merged));
        }
    }
    true
}

fn tool_dedup_key(tool: &Value) -> String {
    let tool_type = tool
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if !tool_type.is_empty() {
        if let Some(name) = tool
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            return format!("type:{tool_type}\u{0}name:{name}");
        }
        if tool_type == "mcp" {
            if let Some(label) = tool
                .get("server_label")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|label| !label.is_empty())
            {
                return format!("type:mcp\u{0}server_label:{label}");
            }
        }
    }
    format!("json:{tool}")
}

/// xAI's untagged enum deserializer refuses a present-but-null `content` on
/// reasoning input items.
fn strip_null_reasoning_content(body: &mut Value) -> bool {
    let Some(input) = body.get_mut("input").and_then(Value::as_array_mut) else {
        return false;
    };
    let mut changed = false;
    for item in input.iter_mut() {
        if item.get("type").and_then(Value::as_str).map(str::trim) != Some("reasoning") {
            continue;
        }
        if let Some(object) = item.as_object_mut() {
            if matches!(object.get("content"), Some(Value::Null)) {
                object.remove("content");
                changed = true;
            }
        }
    }
    changed
}

/// Keeps whitelisted tool types and drops a `tool_choice` that now points at a
/// removed tool.
fn filter_unsupported_tools(body: &mut Value) -> bool {
    let Some(tools) = body.get("tools").and_then(Value::as_array) else {
        return false;
    };
    let filtered: Vec<Value> = tools
        .iter()
        .filter(|tool| {
            tool.get("type")
                .and_then(Value::as_str)
                .map(str::trim)
                .is_some_and(|kind| SUPPORTED_TOOL_TYPES.contains(&kind))
        })
        .cloned()
        .collect();
    let mut changed = filtered.len() != tools.len();
    if changed {
        if let Some(object) = body.as_object_mut() {
            if filtered.is_empty() {
                object.remove("tools");
            } else {
                object.insert("tools".to_string(), Value::Array(filtered.clone()));
            }
        }
    }
    if body.get("tool_choice").is_some() && should_drop_tool_choice(body, &filtered) {
        if let Some(object) = body.as_object_mut() {
            object.remove("tool_choice");
        }
        changed = true;
    }
    changed
}

fn should_drop_tool_choice(body: &Value, tools: &[Value]) -> bool {
    let Some(choice) = body.get("tool_choice") else {
        return false;
    };
    match choice {
        Value::String(_) => false,
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) != Some("function") {
                return false;
            }
            let Some(name) = object.get("name").and_then(Value::as_str) else {
                return false;
            };
            !tools.iter().any(|tool| {
                tool.get("name").and_then(Value::as_str) == Some(name)
                    || tool
                        .get("function")
                        .and_then(|function| function.get("name"))
                        .and_then(Value::as_str)
                        == Some(name)
            })
        }
        _ => false,
    }
}

/// xAI rejects function parameters whose root is a `oneOf`/`anyOf` union (e.g.
/// Codex's built-in `automation_update`); nullable variants collapse to their
/// non-null branch.
fn collapse_union_parameter_schemas(body: &mut Value) -> bool {
    let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut) else {
        return false;
    };
    let mut changed = false;
    for tool in tools.iter_mut() {
        let Some(parameters) = tool.get_mut("parameters") else {
            continue;
        };
        changed |= collapse_root_union(parameters);
    }
    changed
}

fn collapse_root_union(schema: &mut Value) -> bool {
    let Some(object) = schema.as_object_mut() else {
        return false;
    };
    let keyword = ["oneOf", "anyOf"]
        .into_iter()
        .find(|keyword| object.get(*keyword).is_some_and(Value::is_array));
    let Some(keyword) = keyword else {
        return false;
    };
    let variants = object
        .get(keyword)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let non_null: Vec<Value> = variants
        .into_iter()
        .filter(|variant| variant.get("type") != Some(&Value::String("null".into())))
        .collect();
    if non_null.is_empty() {
        object.remove(keyword);
        return true;
    }
    if non_null.len() > 1 {
        object.insert(keyword.to_string(), Value::Array(non_null));
        return true;
    }
    let single = non_null.into_iter().next().expect("len checked above");
    let mut single_object = Map::new();
    if let Some(fields) = single.as_object() {
        for (key, value) in fields {
            single_object.insert(key.clone(), value.clone());
        }
    }
    for (key, value) in object.iter() {
        if key != keyword && !single_object.contains_key(key) {
            single_object.insert(key.clone(), value.clone());
        }
    }
    *schema = Value::Object(single_object);
    true
}

/// Rewrites Codex multi-agent `agent_message` items into ordinary `message`
/// items; xAI's input enum has no such variant. Walks the whole body because
/// collaboration turns can nest them.
fn rewrite_agent_message(body: &mut Value) -> bool {
    rewrite_agent_message_value(body)
}

fn rewrite_agent_message_value(value: &mut Value) -> bool {
    if rewrite_agent_message_item(value) {
        return true;
    }
    match value {
        Value::Array(items) => items.iter_mut().fold(false, |changed, item| {
            rewrite_agent_message_value(item) || changed
        }),
        Value::Object(object) => object.values_mut().fold(false, |changed, child| {
            rewrite_agent_message_value(child) || changed
        }),
        _ => false,
    }
}

fn rewrite_agent_message_item(value: &mut Value) -> bool {
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    if object.get("type").and_then(Value::as_str) != Some("agent_message") {
        return false;
    }
    object.insert("type".to_string(), Value::String("message".into()));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strips_unsupported_fields_and_whitelists_tools() {
        let mut body = json!({
            "model": "grok-4.6",
            "prompt_cache_retention": "24h",
            "safety_identifier": "sid",
            "external_web_access": { "nested": true },
            "tools": [
                { "type": "function", "name": "keep", "parameters": {} },
                { "type": "tool_search", "name": "search" }
            ],
            "input": [
                { "type": "reasoning", "content": null, "summary": [] }
            ]
        });
        assert!(sanitize(&mut body));
        assert!(body.get("prompt_cache_retention").is_none());
        assert!(body.get("safety_identifier").is_none());
        assert!(body.get("external_web_access").is_none());
        let tools = body["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], json!("keep"));
        assert!(body["input"][0].get("content").is_none());
        // Idempotent.
        assert!(!sanitize(&mut body));
    }

    #[test]
    fn grok_45_loses_sampling_knobs_but_others_keep_them() {
        let mut grok45 =
            json!({ "model": "vendor/grok-4.5", "stop": ["\n"], "presence_penalty": 0.1 });
        assert!(sanitize(&mut grok45));
        assert!(grok45.get("stop").is_none());
        let mut other = json!({ "model": "grok-4.6", "stop": ["\n"] });
        sanitize(&mut other);
        assert!(other.get("stop").is_some());
    }

    #[test]
    fn promotes_additional_tools_carriers_and_dedups() {
        let mut body = json!({
            "tools": [{ "type": "function", "name": "existing", "parameters": {} }],
            "input": [
                { "type": "additional_tools", "tools": [
                    { "type": "function", "name": "existing", "parameters": {} },
                    { "type": "function", "name": "carried", "parameters": {} }
                ]},
                { "type": "message", "role": "user", "content": "hi" }
            ]
        });
        assert!(sanitize(&mut body));
        let names: Vec<&str> = body["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert_eq!(names, ["existing", "carried"]);
        assert_eq!(body["input"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn collapses_root_unions_with_null_branches() {
        let mut body = json!({
            "tools": [
                { "type": "function", "name": "automation_update", "parameters": {
                    "oneOf": [
                        { "type": "null" },
                        { "type": "object", "properties": { "prompt": { "type": "string" } }, "additionalProperties": false }
                    ],
                    "description": "update"
                }}
            ]
        });
        assert!(sanitize(&mut body));
        let parameters = &body["tools"][0]["parameters"];
        assert!(parameters.get("oneOf").is_none());
        assert_eq!(parameters["type"], json!("object"));
        assert_eq!(parameters["description"], json!("update"));
        assert!(parameters.get("additionalProperties").is_some());
    }

    #[test]
    fn rewrites_agent_message_items_anywhere() {
        let mut body = json!({
            "input": [
                { "type": "message", "role": "user", "content": "hi" },
                { "type": "agent_message", "role": "assistant", "content": [
                    { "type": "input_text", "text": "child" }
                ]}
            ]
        });
        assert!(rewrite_agent_message(&mut body));
        assert_eq!(body["input"][1]["type"], json!("message"));
        assert_eq!(body["input"][1]["content"][0]["text"], json!("child"));
    }

    #[test]
    fn whole_float_arguments_become_integers_in_responses() {
        let mut response = json!({
            "output": [
                { "type": "function_call", "name": "tool", "call_id": "c1",
                  "arguments": "{\"limit\": 12.0, \"ratio\": 0.5, \"deep\": {\"n\": 3.0}}" }
            ]
        });
        assert!(normalize_integer_arguments(&mut response));
        let arguments = &response["output"][0]["arguments"];
        assert!(arguments.as_str().unwrap().contains("\"limit\":12"));
        assert!(arguments.as_str().unwrap().contains("\"ratio\":0.5"));
        assert!(arguments.as_str().unwrap().contains("\"deep\":{\"n\":3}"));
    }
}
