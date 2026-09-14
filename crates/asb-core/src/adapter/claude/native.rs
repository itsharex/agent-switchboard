//! Native cloud namespaces are claimed only by an explicit native activation.
use super::document::{get, parse};
use crate::{
    adapter::{AdapterError, OverlayEntry},
    claude_native::{self, KINDS, MANIFEST},
    contracts::{ConfigValue, SwitchPlan},
};
use std::collections::BTreeSet;
fn failure(message: impl Into<String>) -> AdapterError {
    AdapterError {
        message: message.into(),
        line: None,
    }
}
pub(super) fn entries(
    current: &str,
    plan: &SwitchPlan,
) -> Result<Vec<(String, OverlayEntry)>, AdapterError> {
    let root = parse(current)?;
    let mut removed = claude_native::declared_keys(&root)
        .map_err(failure)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    for kind in KINDS {
        removed.insert(kind.flag().into());
        removed.insert(kind.base_key().into());
    }
    removed.insert(MANIFEST.into());
    let mut result = Vec::new();
    if let Some(native) = &plan.profile.connection.claude_native {
        if plan.is_gateway() {
            return Err(failure(
                "Claude 原生云 SDK 不能通过本机 Anthropic HTTP 网关投影",
            ));
        }
        let desired = native.projected(plan.profile.base_url.as_deref());
        let mut owned = desired.keys().cloned().collect::<BTreeSet<_>>();
        for key in root
            .get("env")
            .and_then(serde_json::Value::as_object)
            .into_iter()
            .flat_map(|env| env.keys())
        {
            if native.kind.accepts(key) {
                removed.insert(key.clone());
                owned.insert(key.clone());
            }
        }
        for (key, value) in desired {
            removed.remove(&key);
            result.push((
                format!("env.{key}"),
                OverlayEntry::Set(ConfigValue::Str(value)),
            ));
        }
        removed.remove(MANIFEST);
        let manifest = serde_json::to_string(&owned)
            .map_err(|_| failure("Claude 原生配置所有权标记无法编码"))?;
        result.push((
            format!("env.{MANIFEST}"),
            OverlayEntry::Set(ConfigValue::Str(manifest)),
        ));
    }
    result.extend(
        removed
            .into_iter()
            .map(|key| (format!("env.{key}"), OverlayEntry::RemoveIfPresent)),
    );
    Ok(result)
}
pub(super) fn matches(current: &str, plan: &SwitchPlan) -> Result<bool, AdapterError> {
    let root = parse(current)?;
    Ok(entries(current, plan)?
        .iter()
        .all(|(key, entry)| match entry {
            OverlayEntry::Set(ConfigValue::Str(value)) => {
                get(&root, key).and_then(serde_json::Value::as_str) == Some(value.as_str())
            }
            OverlayEntry::RemoveIfPresent => get(&root, key).is_none(),
            _ => true,
        }))
}
pub(super) fn owned_paths(root: &serde_json::Value) -> Result<BTreeSet<String>, AdapterError> {
    let mut keys = claude_native::declared_keys(root)
        .map_err(failure)?
        .into_iter()
        .map(|key| format!("env.{key}"))
        .collect::<BTreeSet<_>>();
    for kind in KINDS {
        keys.insert(format!("env.{}", kind.flag()));
        keys.insert(format!("env.{}", kind.base_key()));
    }
    keys.insert(format!("env.{MANIFEST}"));
    Ok(keys)
}
