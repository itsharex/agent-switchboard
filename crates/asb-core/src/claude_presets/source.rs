//! Claude preset source data and variable expansion; no UI or provider writes are copied.

use crate::{contracts::ProviderAuthBinding, CcSwitchRow};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Catalog {
    pub revision: String,
    pub presets: Vec<SourcePreset>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SourcePreset {
    pub id: String,
    pub name: String,
    pub website_url: String,
    pub api_key_url: Option<String>,
    pub settings_config: Value,
    pub category: Option<String>,
    #[serde(default)]
    pub is_official: bool,
    pub api_key_field: Option<String>,
    pub api_format: Option<String>,
    pub provider_type: Option<String>,
    #[serde(default)]
    pub template_values: BTreeMap<String, TemplateValue>,
    #[serde(default)]
    pub endpoint_candidates: Vec<String>,
    pub models_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateValue {
    pub label: String,
    pub placeholder: String,
    pub default_value: Option<String>,
    pub editor_value: String,
}

pub(super) fn catalog() -> Result<Catalog, String> {
    let catalog: Catalog = serde_json::from_str(include_str!("catalog.json"))
        .map_err(|_| "Claude 离线预设数据无法读取")?;
    if catalog.revision != super::SOURCE_REVISION {
        return Err("Claude 离线预设版本不匹配".into());
    }
    Ok(catalog)
}

impl SourcePreset {
    pub fn unsupported_reason(&self) -> Option<String> {
        None
    }

    pub fn row(
        &self,
        api_key: &str,
        variables: &BTreeMap<String, String>,
        account_id: Option<&str>,
    ) -> Result<CcSwitchRow, String> {
        if let Some(reason) = self.unsupported_reason() {
            return Err(reason);
        }
        if self.provider_type.is_some() && !api_key.is_empty() {
            return Err("Claude 托管预设通过账号绑定认证，不接受供应商 API Key".into());
        }
        if self.provider_type.is_none() && account_id.is_some() {
            return Err("只有 Claude 托管认证预设可以绑定账号".into());
        }
        let mut config = self.settings_config.clone();
        let variables = self.variables(variables)?;
        expand(&mut config, &variables)?;
        if !self.is_official
            && self.provider_type.is_none()
            && !self.template_values.contains_key("AWS_SECRET_ACCESS_KEY")
        {
            if api_key.trim().is_empty() {
                return Err("请填写此 Claude 预设的 API Key".into());
            }
            let env = config
                .get_mut("env")
                .and_then(Value::as_object_mut)
                .ok_or("Claude 预设缺少 env 对象")?;
            let field = self.api_key_field.as_deref().unwrap_or_else(|| {
                if env.contains_key("CLAUDE_CODE_USE_BEDROCK") {
                    return "AWS_BEARER_TOKEN_BEDROCK";
                }
                if env.contains_key("ANTHROPIC_API_KEY") {
                    "ANTHROPIC_API_KEY"
                } else {
                    "ANTHROPIC_AUTH_TOKEN"
                }
            });
            env.insert(field.into(), json!(api_key));
        }
        let mut meta = json!({});
        for (key, value) in [
            ("apiFormat", &self.api_format),
            ("providerType", &self.provider_type),
            ("apiKeyField", &self.api_key_field),
            ("modelsUrl", &self.models_url),
        ] {
            if let Some(value) = value {
                meta[key] = json!(value);
            }
        }
        if let Some(kind) = &self.provider_type {
            meta["authBinding"] = json!(ProviderAuthBinding {
                source: "managed_account".into(),
                auth_provider: Some(kind.clone()),
                account_id: account_id.map(str::to_string)
            });
        }
        let base = config
            .pointer("/env/ANTHROPIC_BASE_URL")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_end_matches('/');
        let endpoints = self
            .endpoint_candidates
            .iter()
            .filter(|url| url.trim_end_matches('/') != base)
            .map(|url| (url.clone(), json!({"url":url,"addedAt":0})))
            .collect::<serde_json::Map<_, _>>();
        if !endpoints.is_empty() {
            meta["customEndpoints"] = Value::Object(endpoints);
        }
        Ok(CcSwitchRow {
            id: self.id.clone(),
            app_type: "claude".into(),
            name: self.name.clone(),
            settings_config: config.to_string(),
            website_url: Some(self.website_url.clone()),
            notes: None,
            display: Some(crate::contracts::ProviderDisplay {
                icon: None,
                icon_color: None,
                category: self.category.clone(),
                created_at: None,
            }),
            meta: Some(meta.to_string()),
        })
    }

    fn variables(
        &self,
        provided: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, String> {
        if provided
            .keys()
            .any(|key| !self.template_values.contains_key(key))
        {
            return Err("Claude 预设包含未声明的模板变量".into());
        }
        self.template_values
            .iter()
            .map(|(key, entry)| {
                let value = provided
                    .get(key)
                    .or(entry.default_value.as_ref())
                    .unwrap_or(&entry.editor_value);
                if value.trim().is_empty()
                    || value != value.trim()
                    || value.len() > 256
                    || value.chars().any(char::is_control)
                {
                    return Err(format!("请填写有效的 Claude 预设变量 {key}"));
                }
                if key == "ENDPOINT_ID"
                    && !value
                        .bytes()
                        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'_'))
                {
                    return Err("ENDPOINT_ID 只能包含字母、数字、下划线和连字符".into());
                }
                Ok((key.clone(), value.clone()))
            })
            .collect()
    }
}

fn expand(value: &mut Value, variables: &BTreeMap<String, String>) -> Result<(), String> {
    match value {
        Value::String(text) => {
            for (key, value) in variables {
                *text = text.replace(&format!("${{{key}}}"), value);
            }
            if text.contains("${") {
                return Err("Claude 预设仍有未填写的模板变量".into());
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                expand(value, variables)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                expand(value, variables)?;
            }
        }
        _ => {}
    }
    Ok(())
}
