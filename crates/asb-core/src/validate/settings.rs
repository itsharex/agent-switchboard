use crate::contracts::{
    AppKind, CodexSubagentKey, CodexSubagentSettings, ConfigValue, SettingValue, SettingsValues,
    MAX_EXACT_CONFIG_INTEGER,
};
use crate::ownership::{setting_spec, setting_specs, SettingControl, SettingOwner, SettingSpec};
use crate::validate::error::ValidationError;

/// Labels used by the sub-agent validation messages.
const SUBAGENT_FIELD_LABELS: [(CodexSubagentKey, &str); 3] = [
    (CodexSubagentKey::Enabled, "多 agent 工具"),
    (CodexSubagentKey::MaxConcurrentThreadsPerSession, "最大并发"),
    (CodexSubagentKey::InterruptMessage, "中断时记录消息"),
];

impl CodexSubagentSettings {
    /// Validates the three global sub-agent runtime intents. Automatic is always
    /// valid: it means the key is not written at all.
    pub fn validate(&self) -> Result<(), ValidationError> {
        for (key, label) in SUBAGENT_FIELD_LABELS {
            let SettingValue::Explicit { value } = self.get(key) else {
                continue;
            };
            let allowed = match key {
                CodexSubagentKey::Enabled | CodexSubagentKey::InterruptMessage => {
                    if matches!(value, ConfigValue::Bool(_)) {
                        continue;
                    }
                    "true 或 false".to_string()
                }
                CodexSubagentKey::MaxConcurrentThreadsPerSession => {
                    if matches!(value, ConfigValue::Number(number)
                        if number.is_finite()
                            && number.fract() == 0.0
                            && *number >= 1.0
                            && *number <= MAX_EXACT_CONFIG_INTEGER as f64)
                    {
                        continue;
                    }
                    format!("不小于 1 且不大于 {MAX_EXACT_CONFIG_INTEGER} 的整数")
                }
            };
            return Err(ValidationError::SubagentBadValue {
                field: label,
                allowed,
                value: value.display(),
            });
        }
        Ok(())
    }
}

impl SettingsValues {
    pub fn validate_client_settings(&self, app: AppKind) -> Result<(), ValidationError> {
        validate_scope(self, app, SettingOwner::Client)
    }

    pub fn validate_provider_parameters(&self, app: AppKind) -> Result<(), ValidationError> {
        validate_scope(self, app, SettingOwner::Provider)
    }
}

fn validate_scope(
    values: &SettingsValues,
    app: AppKind,
    owner: SettingOwner,
) -> Result<(), ValidationError> {
    if !values.claude_extra.is_empty() {
        if app != AppKind::Claude || owner != SettingOwner::Client {
            return Err(ValidationError::ClaudeCommonOptions(
                "Claude 额外通用配置不能进入 Codex 或供应商参数".into(),
            ));
        }
        crate::claude_common::validate(&values.claude_extra)
            .map_err(ValidationError::ClaudeCommonOptions)?;
    }
    for (key, value) in &values.settings {
        if app == AppKind::Codex
            && owner == SettingOwner::Provider
            && key == crate::ownership::CODEX_SUBAGENT_MODEL_KEY
        {
            return Err(ValidationError::RetiredSubagentModelParameter);
        }
        let spec = setting_spec(app, key)
            .filter(|spec| spec.owner == owner && spec.control != SettingControl::None)
            .ok_or_else(|| ValidationError::UnknownSettingKey {
                app,
                key: key.clone(),
            })?;
        if let SettingValue::Explicit { value } = value {
            validate_value(&spec, value)?;
        }
    }
    for spec in setting_specs(app)
        .into_iter()
        .filter(|spec| spec.owner == owner && spec.control != SettingControl::None)
    {
        if !values.settings.contains_key(spec.key) {
            return Err(ValidationError::MissingSettingKey {
                key: spec.key.to_string(),
            });
        }
    }
    Ok(())
}

fn validate_value(spec: &SettingSpec, value: &ConfigValue) -> Result<(), ValidationError> {
    if let ConfigValue::Number(number) = value {
        if !number.is_finite() {
            return Err(ValidationError::NonFiniteNumber {
                key: spec.key.to_string(),
            });
        }
    }
    let allowed = match spec.control {
        SettingControl::Toggle => {
            if matches!(value, ConfigValue::Bool(_)) {
                return Ok(());
            }
            "true 或 false".to_string()
        }
        SettingControl::Choice { .. } => {
            let allowed: Vec<&str> = spec
                .allowed_values
                .iter()
                .map(|option| option.value)
                .collect();
            if matches!(value, ConfigValue::Str(text) if allowed.contains(&text.as_str())) {
                return Ok(());
            }
            allowed.join("、")
        }
        SettingControl::None => unreachable!("scope validation requires an editor control"),
    };
    Err(ValidationError::BadSettingValue {
        key: spec.key.to_string(),
        value: value.display(),
        allowed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::SettingsValues;
    use std::collections::BTreeMap;

    #[test]
    fn a_retired_subagent_parameter_fails_with_its_own_message() {
        let mut settings = BTreeMap::new();
        settings.insert(
            crate::ownership::CODEX_SUBAGENT_MODEL_KEY.to_string(),
            SettingValue::Explicit {
                value: ConfigValue::Str("gpt-5-codex".to_string()),
            },
        );
        let values = SettingsValues {
            settings,
            claude_extra: Default::default(),
        };
        let error = values
            .validate_provider_parameters(AppKind::Codex)
            .expect_err("the retired parameter spelling must fail loudly");
        assert!(matches!(
            error,
            ValidationError::RetiredSubagentModelParameter
        ));
    }
}
