use crate::contracts::{AppKind, CommonSettingValue, CommonSettings, ConfigValue};
use crate::ownership::{setting_spec, SettingControl, SettingOwner};

use crate::validate::error::ValidationError;

impl CommonSettings {
    /// Validates the complete common settings for one client. The key set
    /// must be exactly the ownership directory's common parameters, and every
    /// value must match its control's shape and allowed values.
    pub fn validate_for(&self, app: AppKind) -> Result<(), ValidationError> {
        for (key, setting) in &self.settings {
            let Some(spec) = setting_spec(app, key) else {
                return Err(ValidationError::UnknownCommonKey {
                    app,
                    key: key.clone(),
                });
            };
            if spec.owner != SettingOwner::Common {
                return Err(ValidationError::UnknownCommonKey {
                    app,
                    key: key.clone(),
                });
            }
            let CommonSettingValue::Explicit { value } = setting else {
                continue;
            };
            if let ConfigValue::Number(number) = value {
                if !number.is_finite() {
                    return Err(ValidationError::NonFiniteNumber { key: key.clone() });
                }
            }
            match spec.control {
                SettingControl::Toggle => {
                    if !matches!(value, ConfigValue::Bool(_)) {
                        return Err(ValidationError::BadCommonValue {
                            key: key.clone(),
                            value: value.display(),
                            allowed: "true 或 false".to_string(),
                        });
                    }
                }
                SettingControl::Choice { .. } => {
                    let allowed: Vec<&str> = spec
                        .allowed_values
                        .iter()
                        .map(|option| option.value)
                        .collect();
                    let ok = match value {
                        ConfigValue::Str(text) => allowed.contains(&text.as_str()),
                        _ => false,
                    };
                    if !ok {
                        return Err(ValidationError::BadCommonValue {
                            key: key.clone(),
                            value: value.display(),
                            allowed: allowed.join("、"),
                        });
                    }
                }
                SettingControl::None => {
                    unreachable!("common settings always declare an editor control")
                }
            }
        }
        for spec in crate::ownership::setting_specs(app) {
            if spec.owner == SettingOwner::Common && !self.settings.contains_key(spec.key) {
                return Err(ValidationError::MissingCommonKey {
                    key: spec.key.to_string(),
                });
            }
        }
        Ok(())
    }
}
