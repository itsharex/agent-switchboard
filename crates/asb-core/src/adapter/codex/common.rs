//! Read only the visual common-file keys from a complete live Codex document.
use crate::{
    adapter::AdapterError,
    contracts::{AppKind, SettingValue, SettingsValues},
    ownership,
};
pub fn extract_client_settings(text: &str) -> Result<SettingsValues, AdapterError> {
    let document = super::parse(text)?;
    let mut settings = ownership::default_client_settings(AppKind::Codex);
    for (key, setting) in &mut settings.settings {
        let mut segments = key.split('.');
        let mut item = segments.next().and_then(|key| document.get(key));
        for segment in segments {
            item = item.and_then(|item| item.get(segment));
        }
        if let Some(item) = item {
            let value = item.as_value().ok_or_else(|| AdapterError {
                message: format!("通用配置 {key} 必须是标量值"),
                line: None,
            })?;
            *setting = SettingValue::Explicit {
                value: crate::adapter::client_settings::toml_value(value, key)?,
            };
        }
    }
    settings
        .validate_client_settings(AppKind::Codex)
        .map_err(|e| AdapterError {
            message: e.to_string(),
            line: None,
        })?;
    Ok(settings)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extraction_does_not_copy_routing_mcp_agents_secrets_or_project_overrides() {
        let text = "approval_policy = 'on-request'\nmodel = 'private-model'\n[model_providers.third]\nexperimental_bearer_token = 'secret'\n[mcp_servers.test]\ncommand = 'node'\nargs = ['a','b']\n[projects.'C:/private']\ntrust_level = 'trusted'\n";
        let settings = extract_client_settings(text).unwrap();
        let encoded = serde_json::to_string(&settings).unwrap();
        assert!(encoded.contains("on-request"));
        for value in [
            "private-model",
            "secret",
            "mcp_servers",
            "C:/private",
            "experimental_bearer_token",
        ] {
            assert!(!encoded.contains(value));
        }
    }
    #[test]
    fn malformed_owned_values_fail_instead_of_silently_becoming_automatic() {
        assert!(extract_client_settings("approval_policy = ['on-request']").is_err());
        assert!(extract_client_settings("approval_policy = 'unsupported'").is_err());
    }
}
