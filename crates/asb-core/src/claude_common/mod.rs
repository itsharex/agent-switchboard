//! Additional Claude shared configuration. Visual preferences retain their own keys.
mod pointer;
mod projection;
mod validation;
pub use projection::{apply, changes, diff_documents, fragment, owned_paths};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
pub use validation::validate;
pub const MANIFEST: &str = "ASB_CLAUDE_COMMON_KEYS";
pub type Extra = Map<String, Value>;

/// Separates visual client fields from the additional fragment, without giving
/// either store a second owner for provider or extension configuration.
pub fn split(text: &str) -> Result<(String, Extra), String> {
    let mut extra: Value =
        serde_json::from_str(text).map_err(|_| "Claude 配置片段不是有效 JSON")?;
    if !extra.is_object() {
        return Err("Claude 配置片段根节点必须是对象".into());
    }
    let mut managed = Value::Object(Map::new());
    for spec in crate::ownership::setting_specs(crate::AppKind::Claude) {
        if spec.owner != crate::ownership::SettingOwner::Client
            || spec.control == crate::ownership::SettingControl::None
        {
            continue;
        }
        let segments = spec.key.split('.').map(str::to_string).collect::<Vec<_>>();
        let path = pointer::encode(&segments);
        if let Some(value) = extra.pointer(&path).cloned() {
            pointer::set(&mut managed, &segments, value)?;
            pointer::remove(&mut extra, &segments)?;
        }
    }
    let mut canonical = Value::Object(Map::new());
    for (path, value) in leaves(&extra)? {
        pointer::set(&mut canonical, &pointer::decode(&path)?, value)?;
    }
    let extra = canonical.as_object().expect("object").clone();
    validate(&extra)?;
    Ok((managed.to_string(), extra))
}

fn leaves(value: &Value) -> Result<BTreeMap<String, Value>, String> {
    let mut output = BTreeMap::new();
    collect(value, &mut Vec::new(), &mut output)?;
    Ok(output)
}
fn collect(
    value: &Value,
    path: &mut Vec<String>,
    output: &mut BTreeMap<String, Value>,
) -> Result<(), String> {
    if path.len() > 32 {
        return Err("Claude 通用配置嵌套层级超过 32".into());
    }
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            path.push(key.clone());
            collect(value, path, output)?;
            path.pop();
        }
    } else {
        output.insert(pointer::encode(path), value.clone());
        if output.len() > 512 {
            return Err("Claude 通用配置最多包含 512 个额外字段".into());
        }
    }
    Ok(())
}
fn declared(root: &Value) -> Result<Vec<String>, String> {
    let Some(value) = root.pointer(&format!("/env/{MANIFEST}")) else {
        return Ok(Vec::new());
    };
    let text = value
        .as_str()
        .ok_or("Claude 通用配置所有权标记无效，原文件已保留")?;
    let paths: Vec<String> = serde_json::from_str(text)
        .map_err(|_| "Claude 通用配置所有权标记无法解析，原文件已保留")?;
    if paths.len() > 512 {
        return Err("Claude 通用配置所有权标记超过限制".into());
    }
    for path in &paths {
        validation::path(&pointer::decode(path)?)?;
    }
    Ok(paths)
}
#[cfg(test)]
mod tests;
