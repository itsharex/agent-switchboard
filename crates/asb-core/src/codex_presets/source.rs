use super::{CodexPresetAuthentication, CodexPresetSummary};
use crate::ccswitch::CcSwitchRow;
use crate::contracts::CodexUpstream;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub(super) struct Catalog {
    pub revision: String,
    pub presets: Vec<PresetSource>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PresetSource {
    pub id: String,
    pub name: String,
    website_url: String,
    api_key_url: Option<String>,
    config: String,
    is_official: Option<bool>,
    endpoint_candidates: Option<Vec<String>>,
    api_format: Option<String>,
    provider_type: Option<String>,
    model_catalog: Option<Vec<Value>>,
    codex_chat_reasoning: Option<Value>,
    prompt_cache_routing: Option<String>,
    category: Option<String>,
}

pub(super) fn catalog() -> Result<Catalog, String> {
    let catalog: Catalog = serde_json::from_str(include_str!("catalog.json"))
        .map_err(|_| "内置 Codex 预设目录无效")?;
    if catalog.revision != super::SOURCE_REVISION {
        return Err("内置 Codex 预设版本不匹配".into());
    }
    Ok(catalog)
}

impl PresetSource {
    pub(super) fn authentication(&self) -> CodexPresetAuthentication {
        if self.is_official == Some(true) && self.config.trim().is_empty() {
            CodexPresetAuthentication::NativeOpenAi
        } else if self.provider_type.as_deref() == Some("xai_oauth") {
            CodexPresetAuthentication::ManagedXai
        } else {
            CodexPresetAuthentication::ApiKey
        }
    }

    fn upstream(&self) -> Result<Option<CodexUpstream>, String> {
        if self.is_official == Some(true) && self.config.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(match self.api_format.as_deref() {
            None | Some("openai_responses") => CodexUpstream::Responses,
            Some("openai_chat") => CodexUpstream::ChatCompletions,
            Some("anthropic") => CodexUpstream::AnthropicMessages,
            _ => return Err(format!("Codex 预设协议不受支持：{}", self.name)),
        }))
    }

    pub(super) fn summary(&self) -> Result<CodexPresetSummary, String> {
        let models = self
            .model_catalog
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(|model| model.get("model")?.as_str().map(str::to_string))
            .collect();
        Ok(CodexPresetSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            category: self
                .category
                .clone()
                .unwrap_or_else(|| "third_party".into()),
            website_url: untracked_url(&self.website_url),
            api_key_url: self.api_key_url.as_deref().map(untracked_url),
            authentication: self.authentication(),
            upstream: self.upstream()?,
            endpoint_candidates: self.endpoint_candidates.clone().unwrap_or_default(),
            models,
        })
    }

    pub(super) fn row(&self, api_key: &str) -> Result<CcSwitchRow, String> {
        let mut meta = json!({});
        if let Some(format) = &self.api_format {
            meta["apiFormat"] = json!(format);
        }
        if let Some(reasoning) = self.reasoning() {
            meta["codexChatReasoning"] = reasoning;
        }
        if let Some(mode) = &self.prompt_cache_routing {
            meta["promptCacheRouting"] = json!(mode);
        }
        if let Some(endpoints) = &self.endpoint_candidates {
            meta["customEndpoints"] = Value::Object(
                endpoints
                    .iter()
                    .map(|url| (url.clone(), json!({"url": url, "addedAt": 0})))
                    .collect(),
            );
            meta["endpointAutoSelect"] = json!(false);
        }
        if self.provider_type.as_deref() == Some("xai_oauth") {
            meta["providerType"] = json!("xai_oauth");
        }
        Ok(CcSwitchRow {
            id: self.id.clone(),
            app_type: "codex".into(),
            name: self.name.clone(),
            settings_config: json!({"auth":{"OPENAI_API_KEY": api_key}, "config":self.config,
                "modelCatalog":{"models":self.model_catalog.clone().unwrap_or_default()}})
            .to_string(),
            website_url: Some(untracked_url(&self.website_url)),
            notes: None,
            display: None,
            meta: Some(meta.to_string()),
        })
    }

    fn reasoning(&self) -> Option<Value> {
        if let Some(reasoning) = &self.codex_chat_reasoning {
            return Some(reasoning.clone());
        }
        if self.api_format.as_deref() != Some("openai_chat") {
            return None;
        }
        // Named, pinned preset facts are materialized once, not inferred from an edited endpoint.
        let (thinking, effort, mode) = match self.name.as_str() {
            "StepFun" | "StepFun en" => ("none", "reasoning_effort", "zen"),
            "SiliconFlow en" | "ModelScope" => ("enable_thinking", "none", "passthrough"),
            "BaiLing" => ("thinking", "none", "passthrough"),
            _ => ("none", "none", "passthrough"),
        };
        Some(
            json!({"supportsThinking":thinking!="none", "supportsEffort":effort!="none",
            "thinkingParam":thinking,"effortParam":effort,"effortValueMode":mode}),
        )
    }
}

fn untracked_url(raw: &str) -> String {
    url::Url::parse(raw)
        .map(|mut url| {
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        })
        .unwrap_or_else(|_| raw.to_string())
}
