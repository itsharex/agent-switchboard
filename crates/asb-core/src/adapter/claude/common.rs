//! Read only the visual client-preference keys from live Claude settings.
use crate::{
    adapter::{client_settings::json_value, AdapterError},
    contracts::{AppKind, SettingValue, SettingsValues},
    ownership,
};

pub fn extract_client_settings(text: &str) -> Result<SettingsValues, AdapterError> {
    let document = super::parse(text)?;
    let mut settings = ownership::default_client_settings(AppKind::Claude);
    for (key, setting) in &mut settings.settings {
        if let Some(value) = value_at(&document, key) {
            *setting = SettingValue::Explicit {
                value: json_value(value, key)?,
            };
        }
    }
    settings
        .validate_client_settings(AppKind::Claude)
        .map_err(|error| AdapterError {
            message: error.to_string(),
            line: None,
        })?;
    Ok(settings)
}

fn value_at<'a>(document: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    key.split('.').try_fold(document, |current, segment| current.get(segment))
}
