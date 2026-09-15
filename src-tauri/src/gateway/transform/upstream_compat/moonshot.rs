//! Moonshot / Kimi Chat Completions `$ref` sibling compatibility.
//!
//! Moonshot rejects `$ref` nodes that carry sibling keywords, which Codex's
//! built-in tool schemas do. Each such `$ref` moves into `allOf` for that
//! upstream only; every other provider keeps byte-identical tool schemas so
//! prompt-cache prefixes stay intact.

use serde_json::{json, Map, Value};

const MOONSHOT_HOST_SUFFIXES: &[&str] = &[
    "api.moonshot.cn",
    "api.moonshot.ai",
    "api.kimi.com",
    "moonshot-v1-8k-api.moonshot.cn",
];

/// Schema-valued object keywords whose values are schemas.
const SCHEMA_MAP_KEYWORDS: &[&str] = &[
    "properties",
    "patternProperties",
    "$defs",
    "definitions",
    "dependentSchemas",
    "dependencies",
];

/// Schema-valued keywords whose values are arrays of schemas.
const SCHEMA_ARRAY_KEYWORDS: &[&str] = &["allOf", "anyOf", "oneOf"];

/// Keywords holding exactly one schema (`items` may also be an array).
const SINGLE_SCHEMA_KEYWORDS: &[&str] = &[
    "additionalProperties",
    "contains",
    "propertyNames",
    "if",
    "then",
    "else",
    "not",
    "items",
];

/// Whether the resolved upstream base URL points at a Moonshot / Kimi host.
/// Fails closed on unparsable URLs: no host, no rewrite.
pub(super) fn upstream_requires_ref_sibling_all_of(base_url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(base_url.trim()) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    MOONSHOT_HOST_SUFFIXES.iter().any(|suffix| {
        host == *suffix
            || host
                .strip_suffix(suffix)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

/// Rewrites the serialized Chat Completions body; invalid JSON is an error
/// because silently forwarding a body we could not inspect would hide the
/// upstream failure mode this exists to prevent.
pub(super) fn wrap_ref_siblings_in_chat_tools(body: &mut Vec<u8>) -> Result<(), String> {
    let mut value: Value =
        serde_json::from_slice(body).map_err(|_| "Codex 请求体不是有效 JSON".to_string())?;
    if wrap_in_chat_tools(&mut value) > 0 {
        *body = serde_json::to_vec(&value).map_err(|_| "Codex 请求序列化失败".to_string())?;
    }
    Ok(())
}

/// Rewrites every `$ref`-with-siblings node inside `tools[].function.parameters`.
fn wrap_in_chat_tools(body: &mut Value) -> usize {
    let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut) else {
        return 0;
    };
    let mut changed = 0;
    for tool in tools.iter_mut() {
        if let Some(parameters) = tool
            .get_mut("function")
            .and_then(|function| function.get_mut("parameters"))
        {
            changed += usize::from(wrap_ref_siblings(parameters) > 0);
        }
    }
    changed
}

/// Walks a JSON Schema through schema-valued keywords only and moves every
/// `$ref` with sibling keywords into `allOf`. Data-valued keywords
/// (`default`, `examples`, `enum`, `const`) and unknown `x-…` extensions are
/// never entered, so a literal `$ref` key inside them stays untouched.
pub(super) fn wrap_ref_siblings(schema: &mut Value) -> usize {
    let Value::Object(map) = schema else {
        return 0;
    };
    let mut rewritten = 0;
    if map.len() > 1 && map.get("$ref").is_some_and(Value::is_string) {
        move_ref_into_all_of(map);
        rewritten += 1;
    }
    for (key, child) in map.iter_mut() {
        let key = key.as_str();
        if SCHEMA_MAP_KEYWORDS.contains(&key) {
            if let Value::Object(entries) = child {
                rewritten += entries.values_mut().map(wrap_ref_siblings).sum::<usize>();
            }
        } else if SCHEMA_ARRAY_KEYWORDS.contains(&key) {
            if let Value::Array(entries) = child {
                rewritten += entries.iter_mut().map(wrap_ref_siblings).sum::<usize>();
            }
        } else if SINGLE_SCHEMA_KEYWORDS.contains(&key) {
            match child {
                Value::Array(entries) => {
                    rewritten += entries.iter_mut().map(wrap_ref_siblings).sum::<usize>();
                }
                other => rewritten += wrap_ref_siblings(other),
            }
        }
    }
    rewritten
}

fn move_ref_into_all_of(map: &mut Map<String, Value>) {
    let Some(reference) = map.remove("$ref") else {
        return;
    };
    let branch = json!({ "$ref": reference });
    match map.get_mut("allOf") {
        Some(Value::Array(branches)) => branches.push(branch),
        _ => {
            map.insert("allOf".to_string(), Value::Array(vec![branch]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Codex `automation_update` reduced to the two positions Moonshot
    /// rejected: a property `$ref` with a description and a `$defs` entry
    /// `$ref` with `type`/`minLength` siblings.
    fn desktop_like_schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": { "$ref": "#/$defs/__schema20", "description": "Prompt to run" },
                "mode": { "type": "string", "enum": ["fast", "slow"] }
            },
            "required": ["prompt"],
            "$defs": {
                "__schema20": { "$ref": "#/$defs/__schema2", "type": "string", "minLength": 1 },
                "__schema2": { "type": "string" }
            }
        })
    }

    #[test]
    fn gate_matches_moonshot_and_kimi_hosts_only() {
        for url in [
            "https://api.moonshot.cn/v1",
            "https://api.moonshot.ai/v1/",
            "https://api.kimi.com/coding/v1",
            "https://EU.API.KIMI.COM/coding/v1",
        ] {
            assert!(upstream_requires_ref_sibling_all_of(url), "{url}");
        }
        for url in [
            "https://api.deepseek.com/v1",
            "https://open.bigmodel.cn/api/paas/v4",
            "https://not-moonshot.cn.example.test/v1",
        ] {
            assert!(!upstream_requires_ref_sibling_all_of(url), "{url}");
        }
        assert!(!upstream_requires_ref_sibling_all_of("::::"));
    }

    #[test]
    fn ref_siblings_move_into_allof_only_where_present() {
        let mut body = json!({
            "tools": [
                { "type": "function", "function": { "name": "automation_update", "parameters": desktop_like_schema() } },
                { "type": "function", "function": { "name": "clean", "parameters": { "type": "object" } } }
            ]
        });
        let mut encoded = serde_json::to_vec(&body).unwrap();
        wrap_ref_siblings_in_chat_tools(&mut encoded).unwrap();
        let rewritten: Value = serde_json::from_slice(&encoded).unwrap();
        let parameters = &rewritten["tools"][0]["function"]["parameters"];
        assert_eq!(
            parameters["properties"]["prompt"]["allOf"][0]["$ref"],
            json!("#/$defs/__schema20")
        );
        assert_eq!(
            parameters["properties"]["prompt"]["description"],
            json!("Prompt to run")
        );
        assert_eq!(
            parameters["$defs"]["__schema20"]["allOf"][0]["$ref"],
            json!("#/$defs/__schema2")
        );
        assert_eq!(parameters["$defs"]["__schema20"]["type"], json!("string"));
        // The untouched tool stays byte-identical in shape.
        assert_eq!(
            rewritten["tools"][1]["function"]["parameters"],
            json!({"type": "object"})
        );
    }

    #[test]
    fn data_valued_keywords_are_never_entered() {
        let mut schema = json!({
            "default": { "$ref": "kept-with-sibling", "type": "string" },
            "examples": [{ "$ref": "also-kept" }],
            "properties": { "real": { "$ref": "#/x", "description": "moved" } }
        });
        assert_eq!(wrap_ref_siblings(&mut schema), 1);
        assert!(schema["default"].get("$ref").is_some());
        assert_eq!(schema["examples"][0]["$ref"], json!("also-kept"));
        assert!(schema["properties"]["real"].get("$ref").is_none());
    }
}
