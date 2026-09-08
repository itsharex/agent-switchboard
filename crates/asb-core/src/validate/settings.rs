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
    for (key, value) in &values.settings {
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
        SettingControl::ModelPicker => {
            if matches!(value, ConfigValue::Str(text) if !text.trim().is_empty()) {
                return Ok(());
            }
            "非空模型标识".to_string()
        }
        SettingControl::None => unreachable!("scope validation requires an editor control"),
    };
    Err(ValidationError::BadSettingValue {
        key: spec.key.to_string(),
        value: value.display(),
        allowed,
    })
}
