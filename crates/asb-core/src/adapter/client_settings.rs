//! Parse a standalone client-preference fragment back into typed settings.
//!
//! This deliberately accepts only editor-owned client keys. It never treats a
//! fragment as a complete client file, so provider routing, credentials, and
//! host-owned configuration cannot enter the client-preference store.

use std::collections::BTreeMap;

use toml_edit::{TableLike, Value as TomlValue};

use crate::adapter::{claude, codex, scrub_message, AdapterError};
use crate::contracts::{
    AppKind, ConfigValue, SettingValue, SettingsValues, MAX_EXACT_CONFIG_INTEGER,
};
use crate::ownership::{default_client_settings, setting_spec, SettingControl, SettingOwner};

/// Parses the editable TOML or JSON fragment for one client. Every missing
/// client setting remains automatic; every present value must be an editable
/// client-owned key and pass the same validation as a visual control.
pub fn parse_client_settings(app: AppKind, text: &str) -> Result<SettingsValues, AdapterError> {
    let mut settings = default_client_settings(app);
    if text.trim().is_empty() {
        return Ok(settings);
    }

    let entries = match app {
        AppKind::Codex => toml_entries(text)?,
        AppKind::Claude => {
            let (managed, extra) = crate::claude_common::split(text).map_err(validation_error)?;
            settings.claude_extra = extra;
            json_entries(&managed)?
        }
    };
    for (key, value) in entries {
        let editable = setting_spec(app, &key).is_some_and(|spec| {
            spec.owner == SettingOwner::Client && spec.control != SettingControl::None
        });
        if !editable {
            return Err(unsupported_key(app, &key));
        }
        settings
            .settings
            .insert(key, SettingValue::Explicit { value });
    }
    settings
        .validate_client_settings(app)
        .map_err(validation_error)?;
    Ok(settings)
}

fn unsupported_key(app: AppKind, key: &str) -> AdapterError {
    let message = match setting_spec(app, key) {
        Some(spec) if spec.owner == SettingOwner::Provider => {
            format!("配置片段不能包含供应商设置 {key}")
        }
        _ => format!("配置片段只能包含可编辑的客户端设置，{key} 不受支持"),
    };
    AdapterError {
        message,
        line: None,
    }
}

fn validation_error(error: impl std::fmt::Display) -> AdapterError {
    AdapterError {
        message: scrub_message(error.to_string()),
        line: None,
    }
}

fn toml_entries(text: &str) -> Result<BTreeMap<String, ConfigValue>, AdapterError> {
    let document = codex::parse(text)?;
    let mut entries = BTreeMap::new();
    collect_toml_table(document.as_table(), "", &mut entries)?;
    Ok(entries)
}

fn collect_toml_table(
    table: &dyn TableLike,
    prefix: &str,
    entries: &mut BTreeMap<String, ConfigValue>,
) -> Result<(), AdapterError> {
    for (segment, item) in table.iter() {
        if item.is_none() {
            continue;
        }
        let key = join_key(prefix, segment);
        if let Some(nested) = item.as_table_like() {
            collect_toml_table(nested, &key, entries)?;
            continue;
        }
        let value = item
            .as_value()
            .ok_or_else(|| unsupported_value(&key))
            .and_then(|value| toml_value(value, &key))?;
        entries.insert(key, value);
    }
    Ok(())
}

pub(super) fn toml_value(value: &TomlValue, key: &str) -> Result<ConfigValue, AdapterError> {
    match value {
        TomlValue::Boolean(value) => Ok(ConfigValue::Bool(*value.value())),
        TomlValue::String(value) => Ok(ConfigValue::Str(value.value().clone())),
        TomlValue::Integer(value)
            if (-MAX_EXACT_CONFIG_INTEGER..=MAX_EXACT_CONFIG_INTEGER).contains(value.value()) =>
        {
            Ok(ConfigValue::Number(*value.value() as f64))
        }
        TomlValue::Float(value) if value.value().is_finite() => {
            Ok(ConfigValue::Number(*value.value()))
        }
        _ => Err(unsupported_value(key)),
    }
}

fn json_entries(text: &str) -> Result<BTreeMap<String, ConfigValue>, AdapterError> {
    let document = claude::parse(text)?;
    let mut entries = BTreeMap::new();
    collect_json_object(
        document
            .as_object()
            .expect("Claude document parser guarantees an object root"),
        "",
        &mut entries,
    )?;
    Ok(entries)
}

fn collect_json_object(
    object: &serde_json::Map<String, serde_json::Value>,
    prefix: &str,
    entries: &mut BTreeMap<String, ConfigValue>,
) -> Result<(), AdapterError> {
    for (segment, value) in object {
        let key = join_key(prefix, segment);
        if let serde_json::Value::Object(nested) = value {
            collect_json_object(nested, &key, entries)?;
            continue;
        }
        entries.insert(key.clone(), json_value(value, &key)?);
    }
    Ok(())
}

pub(crate) fn json_value(value: &serde_json::Value, key: &str) -> Result<ConfigValue, AdapterError> {
    match value {
        serde_json::Value::Bool(value) => Ok(ConfigValue::Bool(*value)),
        serde_json::Value::String(value) => Ok(ConfigValue::Str(value.clone())),
        serde_json::Value::Number(value) => json_number(value, key),
        serde_json::Value::Array(_) | serde_json::Value::Null | serde_json::Value::Object(_) => {
            Err(unsupported_value(key))
        }
    }
}

fn json_number(value: &serde_json::Number, key: &str) -> Result<ConfigValue, AdapterError> {
    if let Some(integer) = value.as_i64() {
        return exact_integer(integer, key);
    }
    if let Some(integer) = value.as_u64() {
        return (integer <= MAX_EXACT_CONFIG_INTEGER as u64)
            .then_some(ConfigValue::Number(integer as f64))
            .ok_or_else(|| unsupported_value(key));
    }
    value
        .as_f64()
        .filter(|number| number.is_finite())
        .map(ConfigValue::Number)
        .ok_or_else(|| unsupported_value(key))
}

fn exact_integer(value: i64, key: &str) -> Result<ConfigValue, AdapterError> {
    (-MAX_EXACT_CONFIG_INTEGER..=MAX_EXACT_CONFIG_INTEGER)
        .contains(&value)
        .then_some(ConfigValue::Number(value as f64))
        .ok_or_else(|| unsupported_value(key))
}

fn unsupported_value(key: &str) -> AdapterError {
    AdapterError {
        message: format!("配置片段中的 {key} 必须是受支持的标量值"),
        line: None,
    }
}

fn join_key(prefix: &str, segment: &str) -> String {
    if prefix.is_empty() {
        segment.to_string()
    } else {
        format!("{prefix}.{segment}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::render_client_settings;

    fn explicit_bool(value: bool) -> SettingValue {
        SettingValue::Explicit {
            value: ConfigValue::Bool(value),
        }
    }

    fn explicit_text(value: &str) -> SettingValue {
        SettingValue::Explicit {
            value: ConfigValue::Str(value.to_string()),
        }
    }

    #[test]
    fn rendered_client_fragments_round_trip_to_their_visual_values() {
        let mut codex = default_client_settings(AppKind::Codex);
        codex
            .settings
            .insert("tui.notifications".into(), explicit_bool(true));
        codex
            .settings
            .insert("sandbox_mode".into(), explicit_text("workspace-write"));
        let codex_text = render_client_settings(AppKind::Codex, &codex).expect("render Codex");
        assert_eq!(
            parse_client_settings(AppKind::Codex, &codex_text).unwrap(),
            codex
        );

        let mut claude = default_client_settings(AppKind::Claude);
        claude
            .settings
            .insert("spinnerTipsEnabled".into(), explicit_bool(true));
        claude.settings.insert(
            "preferredNotifChannel".into(),
            explicit_text("terminal_bell"),
        );
        let claude_text = render_client_settings(AppKind::Claude, &claude).expect("render Claude");
        assert_eq!(
            parse_client_settings(AppKind::Claude, &claude_text).unwrap(),
            claude
        );
    }

    #[test]
    fn missing_keys_and_an_empty_fragment_mean_automatic() {
        for app in [AppKind::Codex, AppKind::Claude] {
            assert_eq!(
                parse_client_settings(app, " \n").expect("empty fragment"),
                default_client_settings(app)
            );
        }
    }

    #[test]
    fn parser_rejects_provider_host_and_invalid_client_values() {
        let provider = parse_client_settings(AppKind::Codex, "model = \"gpt-5\"\n")
            .expect_err("provider setting must be rejected");
        assert!(provider.message.contains("供应商设置"));

        let extra = parse_client_settings(AppKind::Claude, r#"{"customSetting":true}"#).unwrap();
        assert_eq!(extra.claude_extra["customSetting"], serde_json::json!(true));

        let invalid = parse_client_settings(AppKind::Codex, "[tui]\nnotifications = \"yes\"\n")
            .expect_err("wrong value type must be rejected");
        assert!(invalid.message.contains("tui.notifications"));
    }
}
