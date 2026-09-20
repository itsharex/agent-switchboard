use crate::adapter::AdapterError;

/// Restores accept the current built-in provider contract without migrating a snapshot.
/// Unselected user-owned provider tables remain ordinary preserved configuration.
pub fn validate_restore_configuration(configuration: &str) -> Result<(), AdapterError> {
    let document = super::document::parse(configuration)?;
    let provider = document
        .get("model_provider")
        .map(|item| item.as_str())
        .unwrap_or(Some("openai"));
    let retired_table = document.get("model_providers").is_some_and(|providers| {
        ["openai", "agent_switchboard", "OpenAi"]
            .iter()
            .any(|id| providers.get(*id).is_some())
    });
    if provider != Some("openai") || retired_table {
        return Err(AdapterError {
            message: "备份不符合当前 Codex openai 配置契约；不支持恢复旧供应商格式，请重新应用供应商".into(),
            line: None,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_restore_configuration;

    #[test]
    fn accepts_current_routes_and_preserves_unselected_user_tables() {
        for configuration in [
            "",
            "model_provider = 'openai'",
            "openai_base_url = 'https://relay.example/v1'",
            "model_provider = 'openai'\n[model_providers.personal]\nbase_url = 'https://user.example/v1'",
        ] {
            assert!(validate_restore_configuration(configuration).is_ok());
        }
    }

    #[test]
    fn rejects_retired_provider_shapes_without_migration() {
        for configuration in [
            "model_provider = 'personal'",
            "model_provider = 1",
            "[model_providers.openai]\nbase_url = 'https://relay.example/v1'",
            "[model_providers.agent_switchboard]\nbase_url = 'https://relay.example/v1'",
            "[model_providers.OpenAi]\nbase_url = 'https://relay.example/v1'",
        ] {
            assert!(validate_restore_configuration(configuration).is_err());
        }
    }
}
