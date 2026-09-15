use serde_json::{json, Value};
pub(super) fn required(endpoint: &str) -> bool {
    reqwest::Url::parse(endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .is_some_and(|host| {
            ["moonshot.cn", "moonshot.ai", "kimi.com"]
                .iter()
                .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
        })
}
pub(super) fn rewrite(body: &mut Value) -> Result<bool, String> {
    let mut changed = false;
    for tool in body
        .get_mut("tools")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if let Some(schema) = tool.pointer_mut("/function/parameters") {
            changed |= schema_refs(schema)?;
        }
    }
    Ok(changed)
}
/// Only schema-valued keywords are traversed; examples/default/enum remain literal data.
fn schema_refs(schema: &mut Value) -> Result<bool, String> {
    let Some(root) = schema.as_object_mut() else {
        return Ok(false);
    };
    let mut changed = false;
    for key in [
        "properties",
        "patternProperties",
        "$defs",
        "definitions",
        "dependentSchemas",
        "dependencies",
    ] {
        for child in root
            .get_mut(key)
            .and_then(Value::as_object_mut)
            .into_iter()
            .flat_map(|map| map.values_mut())
        {
            changed |= schema_refs(child)?;
        }
    }
    for key in [
        "items",
        "additionalItems",
        "unevaluatedItems",
        "contains",
        "additionalProperties",
        "unevaluatedProperties",
        "propertyNames",
        "not",
        "if",
        "then",
        "else",
        "contentSchema",
        "allOf",
        "anyOf",
        "oneOf",
        "prefixItems",
    ] {
        if let Some(value) = root.get_mut(key) {
            if let Some(items) = value.as_array_mut() {
                for child in items {
                    changed |= schema_refs(child)?;
                }
            } else {
                changed |= schema_refs(value)?;
            }
        }
    }
    if root.len() > 1 && root.get("$ref").is_some_and(Value::is_string) {
        if root.get("allOf").is_some_and(|value| !value.is_array()) {
            return Err("Moonshot 工具 schema 的 allOf 必须是数组，未丢弃原有约束".into());
        }
        let reference = root.remove("$ref").unwrap();
        root.entry("allOf")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap()
            .insert(0, json!({"$ref":reference}));
        changed = true;
    }
    Ok(changed)
}
