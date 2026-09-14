use super::*;
use crate::contracts::{ChangeKind, KeyChange};
use std::collections::BTreeSet;
fn parse(text: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(text).map_err(|_| "Claude 配置不是有效 JSON")?;
    if !value.is_object() {
        return Err("Claude 配置根节点必须是对象".into());
    }
    Ok(value)
}
pub fn apply(text: &str, extra: &Extra) -> Result<String, String> {
    validate(extra)?;
    let mut root = parse(text)?;
    let previous = declared(&root)?;
    let desired = leaves(&Value::Object(extra.clone()))?;
    for path in previous {
        if !desired.contains_key(&path) {
            pointer::remove(&mut root, &pointer::decode(&path)?)?;
        }
    }
    for (path, value) in &desired {
        pointer::set(&mut root, &pointer::decode(path)?, value.clone())?;
    }
    let path = vec!["env".into(), MANIFEST.into()];
    if desired.is_empty() {
        pointer::remove(&mut root, &path)?;
    } else {
        let keys = serde_json::to_string(&desired.keys().collect::<Vec<_>>())
            .map_err(|_| "Claude 通用配置标记无法编码")?;
        pointer::set(&mut root, &path, Value::String(keys))?;
    }
    serde_json::to_string_pretty(&root).map_err(|_| "Claude 通用配置无法编码".into())
}
pub fn fragment(text: &str, extra: &Extra) -> Result<String, String> {
    validate(extra)?;
    let mut root = parse(text)?;
    for (path, value) in leaves(&Value::Object(extra.clone()))? {
        pointer::set(&mut root, &pointer::decode(&path)?, value)?;
    }
    serde_json::to_string_pretty(&root).map_err(|_| "Claude 通用片段无法编码".into())
}
pub fn owned_paths(root: &Value) -> Result<BTreeSet<String>, String> {
    let mut paths = declared(root)?.into_iter().collect::<BTreeSet<_>>();
    paths.insert(format!("/env/{MANIFEST}"));
    Ok(paths)
}
pub fn changes(before: &str, extra: &Extra) -> Result<Vec<KeyChange>, String> {
    let after = parse(&apply(before, extra)?)?;
    let before = parse(before)?;
    let mut paths = owned_paths(&before)?;
    paths.extend(owned_paths(&after)?);
    Ok(paths
        .into_iter()
        .filter_map(|path| {
            let old = before.pointer(&path);
            let new = after.pointer(&path);
            if old == new {
                return None;
            }
            Some(KeyChange {
                key: path.clone(),
                kind: if new.is_some() {
                    ChangeKind::Set
                } else {
                    ChangeKind::Remove
                },
                before: old.map(|v| display(&path, v)),
                after: new.map(|v| display(&path, v)),
            })
        })
        .collect())
}
pub fn display(path: &str, value: &Value) -> String {
    redacted(path, value).to_string()
}
fn redacted(path: &str, value: &Value) -> Value {
    let credential_url = value
        .as_str()
        .and_then(|value| url::Url::parse(value).ok())
        .is_some_and(|url| !url.username().is_empty() || url.password().is_some());
    if credential_url
        || crate::redact::is_secret_key(path)
        || value.as_str().is_some_and(crate::redact::is_secret_value)
    {
        return Value::String(crate::redact::REDACTED.into());
    }
    match value {
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), redacted(&format!("{path}/{key}"), value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .enumerate()
                .map(|(i, value)| redacted(&format!("{path}/{i}"), value))
                .collect(),
        ),
        _ => value.clone(),
    }
}

pub fn diff_documents(current: &str, previous: &str) -> Result<Vec<KeyChange>, String> {
    let current = parse(current)?;
    let previous = parse(previous)?;
    let mut paths = owned_paths(&current)?;
    paths.extend(owned_paths(&previous)?);
    Ok(paths
        .into_iter()
        .filter_map(|path| {
            let before = previous.pointer(&path);
            let after = current.pointer(&path);
            (before != after).then(|| KeyChange {
                key: path.clone(),
                kind: if after.is_some() {
                    ChangeKind::Set
                } else {
                    ChangeKind::Remove
                },
                before: before.map(|v| display(&path, v)),
                after: after.map(|v| display(&path, v)),
            })
        })
        .collect())
}
