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
fn apply_with(text: &str, extra: &Extra, manifest: &str) -> Result<String, String> {
    validate(extra)?;
    let mut root = parse(text)?;
    let previous = declared(&root, manifest)?;
    let desired = leaves(&Value::Object(extra.clone()))?;
    for path in previous {
        if !desired.contains_key(&path) {
            remove_pruning(&mut root, &pointer::decode(&path)?)?;
        }
    }
    for (path, value) in &desired {
        pointer::set(&mut root, &pointer::decode(path)?, value.clone())?;
    }
    let path = vec!["env".into(), manifest.into()];
    if desired.is_empty() {
        remove_pruning(&mut root, &path)?;
    } else {
        let keys = serde_json::to_string(&desired.keys().collect::<Vec<_>>())
            .map_err(|_| "Claude 通用配置标记无法编码")?;
        pointer::set(&mut root, &path, Value::String(keys))?;
    }
    serde_json::to_string_pretty(&root).map_err(|_| "Claude 通用配置无法编码".to_string())
}

/// Removes one leaf and every parent container it emptied, so switching a
/// fragment away leaves no dangling empty objects behind.
fn remove_pruning(root: &mut Value, segments: &[String]) -> Result<(), String> {
    pointer::remove(root, segments)?;
    for depth in (0..segments.len().saturating_sub(1)).rev() {
        let parent = pointer::encode(&segments[..=depth]);
        let empty = root
            .pointer(&parent)
            .is_some_and(|node| node.as_object().is_some_and(Map::is_empty));
        if empty {
            pointer::remove(root, &segments[..=depth])?;
        } else {
            break;
        }
    }
    Ok(())
}
pub fn apply(text: &str, extra: &Extra) -> Result<String, String> {
    apply_with(text, extra, MANIFEST)
}
/// Applies one provider profile's fragment through the profile-owned
/// manifest, so switching profiles swaps the fragment wholesale while
/// unowned keys stay untouched.
pub fn apply_profile(text: &str, fragment: &Extra) -> Result<String, String> {
    apply_with(text, fragment, PROFILE_MANIFEST)
}
/// Rejects one path claimed by both the client's shared extra and the active
/// profile's fragment. Ownership would otherwise flip between the two stores
/// on every switch.
pub fn validate_scopes(global: &Extra, fragment: &Extra) -> Result<(), String> {
    let shared = leaves(&Value::Object(global.clone()))?
        .keys()
        .map(|path| pointer::decode(path))
        .collect::<Result<Vec<Vec<String>>, _>>()?;
    for path in leaves(&Value::Object(fragment.clone()))?.keys() {
        let segments = pointer::decode(path)?;
        let overlaps =
            |other: &Vec<String>| segments.starts_with(other) || other.starts_with(&segments);
        if shared.iter().any(overlaps) {
            return Err(format!("配置片段不能包含通用配置已管理的 {path}"));
        }
    }
    Ok(())
}

/// Keeps only source entries that may legally live in a fragment; ownership-
/// or shape-rejected entries are reported by name instead of silently
/// disappearing. `env` entries filter per name, everything else per top-level
/// key, matching how import warnings are grouped.
pub fn import_filter(candidate: &Extra) -> (Extra, Vec<String>) {
    let mut kept = Extra::new();
    let mut dropped = Vec::new();
    // Validation runs on one entry at a time so a single bad key never drops
    // its innocent neighbours.
    let entry = |key: &str, value: &Value| -> bool {
        let mut clone = Extra::new();
        clone.insert(key.to_string(), value.clone());
        validate(&clone).is_ok()
    };
    for (key, value) in candidate {
        if key == "env" {
            let mut env = Map::new();
            match value.as_object() {
                Some(source) => {
                    for (name, entry_value) in source {
                        // Validate inside `env` so the string-value rule applies.
                        let mut probe = Extra::new();
                        probe.insert(
                            "env".into(),
                            Value::Object(
                                [(name.clone(), entry_value.clone())].into_iter().collect(),
                            ),
                        );
                        if validate(&probe).is_ok() {
                            env.insert(name.clone(), entry_value.clone());
                        } else {
                            dropped.push(format!("env.{name}"));
                        }
                    }
                }
                None => dropped.push(key.clone()),
            }
            if !env.is_empty() {
                kept.insert("env".into(), Value::Object(env));
            }
            continue;
        }
        if entry(key, value) {
            kept.insert(key.clone(), value.clone());
        } else {
            dropped.push(key.clone());
        }
    }
    (kept, dropped)
}
pub fn owned_paths(root: &Value) -> Result<BTreeSet<String>, String> {
    let mut paths = declared(root, MANIFEST)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    paths.extend(declared(root, PROFILE_MANIFEST)?);
    paths.insert(format!("/env/{MANIFEST}"));
    paths.insert(format!("/env/{PROFILE_MANIFEST}"));
    Ok(paths)
}

/// Same claim set as [`owned_paths`], rendered as dotted document paths so
/// the adapter layer can match leaves without knowing pointer encoding.
pub fn owned_dotted_paths(root: &Value) -> Result<BTreeSet<String>, String> {
    owned_paths(root)?
        .into_iter()
        .map(|path| pointer::decode(&path).map(|segments| segments.join(".")))
        .collect()
}
fn changes_with(before: &str, extra: &Extra, manifest: &str) -> Result<Vec<KeyChange>, String> {
    let after = parse(&apply_with(before, extra, manifest)?)?;
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
pub fn changes(before: &str, extra: &Extra) -> Result<Vec<KeyChange>, String> {
    changes_with(before, extra, MANIFEST)
}
/// Preview-time diff for one provider profile's fragment; only profile-owned
/// manifest paths can appear.
pub fn changes_profile(before: &str, fragment: &Extra) -> Result<Vec<KeyChange>, String> {
    changes_with(before, fragment, PROFILE_MANIFEST)
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
