use super::{CodexBilling, CodexModelPrice};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CodexMeteringSettings {
    pub version: u8,
    pub prices: BTreeMap<String, CodexModelPrice>,
    pub providers: BTreeMap<String, CodexBilling>,
}
impl Default for CodexMeteringSettings {
    fn default() -> Self {
        Self {
            version: 1,
            prices: BTreeMap::new(),
            providers: BTreeMap::new(),
        }
    }
}
impl CodexMeteringSettings {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.prices.len() > 10_000 || self.providers.len() > 1000 {
            return Err("Codex 计量配置版本或条目数量无效".into());
        }
        for (model, price) in &self.prices {
            if model.trim() != model
                || model.is_empty()
                || model.len() > 512
                || model.chars().any(char::is_control)
            {
                return Err("Codex 价格表的模型名称无效".into());
            }
            price.validate()?;
        }
        for (id, billing) in &self.providers {
            if uuid::Uuid::parse_str(id).is_err() {
                return Err("Codex 计量配置供应商标识无效".into());
            }
            billing.validate()?;
        }
        Ok(())
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexMeteringSnapshot {
    pub settings: CodexMeteringSettings,
    pub revision: String,
}
pub(crate) fn read_settings(root: &Path) -> Result<CodexMeteringSnapshot, String> {
    let text = crate::config_store::read_optional(&root.join("codex/metering.json"))
        .map_err(|error| format!("Codex 计量配置不可读：{error}"))?;
    let settings = match &text {
        Some(text) => crate::config_store::parse_strict(text)
            .map_err(|_| "Codex 计量配置格式无效；原文件未更改")?,
        None => CodexMeteringSettings::default(),
    };
    settings.validate()?;
    Ok(CodexMeteringSnapshot {
        settings,
        revision: asb_switch::sha256_hex(text.as_deref().unwrap_or("")),
    })
}
