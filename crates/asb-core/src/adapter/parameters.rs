use crate::adapter::{claude, codex, scrub_message, AdapterError};
use crate::contracts::{AppKind, ConfigValue, SettingValue, SettingsValues};
use crate::ownership::default_provider_parameters;

/// Captures only provider-owned scalar parameters. Missing keys retain their
/// actual automatic meaning; present values must be representable exactly.
pub fn read_provider_parameters(app: AppKind, text: &str) -> Result<SettingsValues, AdapterError> {
    match app {
        AppKind::Codex => {
            let document = codex::parse(text)?;
            collect_parameters(app, |key| toml_value(&document, key))
        }
        AppKind::Claude => {
            let document = claude::parse(text)?;
            collect_parameters(app, |key| json_value(&document, key))
        }
    }
}

fn collect_parameters(
    app: AppKind,
    mut read: impl FnMut(&str) -> Result<Option<ConfigValue>, AdapterError>,
) -> Result<SettingsValues, AdapterError> {
    let mut parameters = default_provider_parameters(app);
    for (key, setting) in &mut parameters.settings {
        if let Some(value) = read(key)? {
            *setting = SettingValue::Explicit { value };
        }
    }
    parameters
        .validate_provider_parameters(app)
        .map_err(|error| AdapterError {
            message: scrub_message(error.to_string()),
            line: None,
        })?;
    Ok(parameters)
}

fn toml_value(
    document: &toml_edit::DocumentMut,
    key: &str,
) -> Result<Option<ConfigValue>, AdapterError> {
    let mut current = document.as_item();
    for segment in key.split('.') {
        let table = current
            .as_table_like()
            .ok_or_else(|| invalid_parameter(key))?;
        let Some(value) = table.get(segment) else {
            return Ok(None);
        };
        current = value;
    }
    let value = match current.as_value() {
        Some(toml_edit::Value::Boolean(value)) => ConfigValue::Bool(*value.value()),
        Some(toml_edit::Value::String(value)) => ConfigValue::Str(value.value().clone()),
        _ => return Err(invalid_parameter(key)),
    };
    Ok(Some(value))
}

fn json_value(
    document: &serde_json::Value,
    key: &str,
) -> Result<Option<ConfigValue>, AdapterError> {
    let mut current = document;
    for segment in key.split('.') {
        let object = current.as_object().ok_or_else(|| invalid_parameter(key))?;
        let Some(value) = object.get(segment) else {
            return Ok(None);
        };
        current = value;
    }
    let value = match current {
        serde_json::Value::Bool(value) => ConfigValue::Bool(*value),
        serde_json::Value::String(value) => ConfigValue::Str(value.clone()),
        _ => return Err(invalid_parameter(key)),
    };
    Ok(Some(value))
}

fn invalid_parameter(key: &str) -> AdapterError {
    AdapterError {
        message: format!("供应商参数 {key} 的当前结构无法表示为受支持的设置值"),
        line: None,
    }
}
