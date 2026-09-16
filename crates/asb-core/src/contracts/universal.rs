//! The universal connection: one provider definition authored once and fanned
//! out into per-app projections. Codex is the only consumer in this product;
//! the entity owns the shared connection and model facts while every
//! generated profile keeps its own store, identity, and local overrides. The
//! two profile contracts are never merged here.

use serde::{Deserialize, Serialize};

use super::{
    AppKind, AuthenticationScheme, CodexCatalogEntry, CodexEndpoint, CodexProviderDraft,
    CodexUpstream, ResponsesRequestMode, DEFAULT_CODEX_CAPABILITIES,
};
use crate::ownership::default_provider_parameters;

/// One shared connection definition. `generated_codex_profile_id` is the only
/// link to the specialized Codex store; deleting either side leaves the other
/// independent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UniversalProvider {
    pub id: String,
    pub name: String,
    pub endpoint: CodexEndpoint,
    pub upstream: CodexUpstream,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub authentication: Option<AuthenticationScheme>,
    pub default_model: String,
    pub catalog: Vec<CodexCatalogEntry>,
    #[serde(default)]
    pub generated_codex_profile_id: Option<String>,
}

/// A create/update request from the command layer. The store turns it into a
/// stored [`UniversalProvider`]; `catalog` is derived from bare model ids so
/// callers never hand-craft capability facts.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UniversalProviderInput {
    pub name: String,
    pub endpoint: String,
    pub upstream: CodexUpstream,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub authentication: Option<AuthenticationScheme>,
    /// The first model is the shared default.
    pub models: Vec<String>,
}

impl UniversalProviderInput {
    /// Validates the shared connection facts and builds the catalog with the
    /// single default-entry semantics shared with live-config import.
    pub fn into_provider(self, id: String) -> Result<UniversalProvider, String> {
        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err("通用连接名称不能为空".into());
        }
        let mut url = url::Url::parse(self.endpoint.trim())
            .map_err(|_| "通用连接服务地址无效".to_string())?;
        if matches!(
            self.upstream,
            CodexUpstream::Responses | CodexUpstream::ChatCompletions
        ) && url.path().trim_matches('/').is_empty()
        {
            url.set_path("/v1");
        }
        crate::endpoint::validate_base_url(url.as_str(), self.upstream.protocol())
            .map_err(|_| "通用连接服务地址无效".to_string())?;
        let endpoint = url.to_string().trim_end_matches('/').to_string();
        let mut models: Vec<String> = Vec::new();
        for model in &self.models {
            let model = model.trim();
            if model.is_empty() {
                continue;
            }
            if !models.iter().any(|existing| existing == model) {
                models.push(model.to_string());
            }
        }
        let default_model = models
            .first()
            .cloned()
            .ok_or_else(|| "通用连接至少需要一个模型".to_string())?;
        Ok(UniversalProvider {
            id,
            name,
            endpoint: CodexEndpoint(endpoint),
            upstream: self.upstream,
            api_key: self.api_key.trim().to_string(),
            authentication: self.authentication,
            default_model,
            catalog: models
                .iter()
                .map(|model| CodexCatalogEntry::default_entry(model))
                .collect(),
            generated_codex_profile_id: None,
        })
    }
}

impl UniversalProvider {
    /// The shared connection and model facts must be internally consistent
    /// before any projection may consume them.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("通用连接名称不能为空".into());
        }
        crate::endpoint::validate_base_url(&self.endpoint.0, self.upstream.protocol())
            .map_err(|_| "通用连接服务地址无效".to_string())?;
        if self.default_model.trim().is_empty() {
            return Err("通用连接缺少默认模型".into());
        }
        if !self
            .catalog
            .iter()
            .any(|entry| entry.id == self.default_model)
        {
            return Err("通用连接的默认模型不在模型目录中".into());
        }
        Ok(())
    }

    /// CC 的 `UniversalProvider::to_codex_provider` 对应物：连接、凭据与目录
    /// 是通用事实；本端能力声明、参数与路由保持档案本地语义，新建时取默认。
    pub fn to_codex_provider(&self) -> CodexProviderDraft {
        CodexProviderDraft {
            name: self.name.clone(),
            endpoint: self.endpoint.clone(),
            api_key: self.api_key.clone(),
            authentication: self.authentication,
            connection: Default::default(),
            upstream: self.upstream,
            request_mode: ResponsesRequestMode::Standard,
            default_model: self.default_model.clone(),
            catalog: self.catalog.clone(),
            model_routes: Vec::new(),
            capabilities: DEFAULT_CODEX_CAPABILITIES,
            parameters: default_provider_parameters(AppKind::Codex),
            notes: None,
            website_url: None,
            usage_query: None,
        }
    }
}
