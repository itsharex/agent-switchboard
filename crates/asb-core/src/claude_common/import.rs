//! Extracting a profile fragment from source settings documents.
use super::{import_filter, Extra};
use crate::contracts::{AppKind, SettingsValues};
use serde_json::Value;

/// Extracts settings.json entries the structured profile fields cannot
/// express into a profile-owned fragment. `reserved` lists extra top-level
/// keys the caller's own structured mapping consumed. Env names owned by the
/// provider directory or the native cloud SDK belong to their owners;
/// everything else is filtered through fragment validation so rejected keys
/// surface as named import losses instead of silently disappearing.
/// Claude settings env values are strings; sources may carry booleans or
/// numbers (upstream presets do). Scalar coercion preserves the source's
/// effective toggle value instead of dropping it, and mirrors how upstream's
/// own quick toggles write these keys as strings.
pub(super) fn env_text(value: &Value) -> Option<Value> {
    match value {
        Value::String(_) => Some(value.clone()),
        Value::Bool(flag) => Some(Value::String(flag.to_string())),
        Value::Number(number) => Some(Value::String(number.to_string())),
        _ => None,
    }
}

pub fn import_fragment(
    config: &Value,
    parameters: &SettingsValues,
    reserved: &[&str],
) -> (Extra, Vec<String>) {
    let mut candidate = Extra::new();
    let native_accepts = |name: &str| {
        crate::claude_native::from_config(config)
            .ok()
            .flatten()
            .is_some_and(|(native, _)| {
                native.kind.accepts(name)
                    || name == native.kind.flag()
                    || name == native.kind.base_key()
            })
    };
    let mut unsupported = Vec::new();
    if let Some(Value::Object(env)) = config.get("env") {
        let mut extra_env = serde_json::Map::new();
        for (name, value) in env {
            let owned =
                crate::ownership::is_provider_owned(AppKind::Claude, &format!("env.{name}"));
            if owned || native_accepts(name) {
                continue;
            }
            match env_text(value) {
                Some(text) => {
                    extra_env.insert(name.clone(), text);
                }
                None => unsupported.push(format!("env.{name}")),
            }
        }
        if !extra_env.is_empty() {
            candidate.insert("env".into(), Value::Object(extra_env));
        }
    }
    for (name, value) in config.as_object().into_iter().flatten() {
        let consumed = name == "env"
            || reserved.contains(&name.as_str())
            || crate::ownership::is_provider_owned(AppKind::Claude, name)
            || parameters.settings.contains_key(name);
        if !consumed {
            candidate.insert(name.clone(), value.clone());
        }
    }
    let (kept, mut dropped) = import_filter(&candidate);
    dropped.extend(unsupported);
    (kept, dropped)
}
