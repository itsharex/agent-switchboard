//! Adds provider-owned Codex sub-agent defaults to the immediately preceding
//! provider-file contract. Both fields must move together because they share
//! the same provider selection boundary.

use super::{layout, ConfigStore};
use asb_core::contracts::AppKind;
use asb_core::ownership::{
    default_provider_parameters, CODEX_SUBAGENT_MODEL_KEY, CODEX_SUBAGENT_REASONING_EFFORT_KEY,
};
use serde_json::{Map, Value};

const KEYS: [&str; 2] = [
    CODEX_SUBAGENT_MODEL_KEY,
    CODEX_SUBAGENT_REASONING_EFFORT_KEY,
];

pub(super) fn needs_upgrade(store: &ConfigStore) -> Result<bool, String> {
    let mut previous = false;
    let mut current = false;
    for value in layout::provider_values(store, AppKind::Codex)? {
        let Some(parameters) = value.get("parameters") else {
            // The layout migration owns profiles without a parameters field.
            continue;
        };
        let settings = parameters
            .get("settings")
            .and_then(Value::as_object)
            .ok_or_else(|| "供应商运行参数不是对象，已取消升级".to_string())?;
        match (
            settings.contains_key(CODEX_SUBAGENT_MODEL_KEY),
            settings.contains_key(CODEX_SUBAGENT_REASONING_EFFORT_KEY),
        ) {
            (false, false) => previous = true,
            (true, true) => current = true,
            _ => {
                return Err("子 agent 默认模型和推理强度必须同时存在，已取消升级".to_string());
            }
        }
    }
    if previous && current {
        return Err("配置目录混合了升级前后的子 agent 参数，原数据保持不变".to_string());
    }
    Ok(previous)
}

pub(crate) fn add_defaults(app: AppKind, value: &mut Value) -> Result<(), String> {
    if app != AppKind::Codex {
        return Ok(());
    }
    let settings = settings_mut(value)?;
    let has_model = settings.contains_key(CODEX_SUBAGENT_MODEL_KEY);
    let has_effort = settings.contains_key(CODEX_SUBAGENT_REASONING_EFFORT_KEY);
    match (has_model, has_effort) {
        (true, true) => return Ok(()),
        (true, false) | (false, true) => {
            return Err("子 agent 默认模型和推理强度必须同时存在，已取消升级".to_string());
        }
        (false, false) => {}
    }

    let defaults = default_provider_parameters(AppKind::Codex);
    for key in KEYS {
        let default = defaults
            .settings
            .get(key)
            .ok_or_else(|| format!("缺少子 agent 参数默认值：{key}"))?;
        settings.insert(
            key.to_string(),
            serde_json::to_value(default).map_err(|error| error.to_string())?,
        );
    }
    Ok(())
}

fn settings_mut(value: &mut Value) -> Result<&mut Map<String, Value>, String> {
    value
        .get_mut("parameters")
        .and_then(Value::as_object_mut)
        .and_then(|parameters| parameters.get_mut("settings"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "供应商运行参数不是对象，已取消升级".to_string())
}
