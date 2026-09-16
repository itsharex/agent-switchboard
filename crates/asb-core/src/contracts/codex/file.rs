use super::{
    codex_model_catalog_document, CodexProviderDraft, CodexProviderFile, CodexProviderProfile,
    CodexProviderRecord, CodexRouteMode, CodexUpstream, CODEX_PROVIDER_SCHEMA_VERSION,
};
use crate::contracts::{
    CodexModelSettings, ExplicitMaxOutputTokens, ModelOptions, ProviderFile, ResponsesOptions,
    RouteMode,
};

impl CodexProviderDraft {
    pub fn into_file(self, id: String, position: u64) -> CodexProviderFile {
        CodexProviderFile {
            schema_version: CODEX_PROVIDER_SCHEMA_VERSION,
            position,
            profile: CodexProviderProfile {
                id,
                name: self.name,
                route_mode: CodexRouteMode::for_connection(
                    self.upstream,
                    &self.connection,
                    self.authentication,
                ),
                endpoint: self.endpoint,
                api_key: self.api_key,
                authentication: self.authentication,
                connection: self.connection,
                upstream: self.upstream,
                request_mode: self.request_mode,
                default_model: self.default_model,
                catalog: self.catalog,
                model_routes: self.model_routes,
                capabilities: self.capabilities,
            },
            parameters: self.parameters,
            notes: self.notes,
            website_url: self.website_url,
            usage_query: self.usage_query,
        }
    }
}

impl CodexProviderRecord {
    pub fn into_draft(self) -> CodexProviderDraft {
        CodexProviderDraft {
            name: self.profile.name,
            endpoint: self.profile.endpoint,
            api_key: self.profile.api_key,
            authentication: self.profile.authentication,
            connection: self.profile.connection,
            upstream: self.profile.upstream,
            request_mode: self.profile.request_mode,
            default_model: self.profile.default_model,
            catalog: self.profile.catalog,
            model_routes: self.profile.model_routes,
            capabilities: self.profile.capabilities,
            parameters: self.parameters,
            notes: self.notes,
            website_url: self.website_url,
            usage_query: self.usage_query,
        }
    }
}

impl CodexProviderFile {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != CODEX_PROVIDER_SCHEMA_VERSION {
            return Err("Codex 供应商档案版本不受支持，请重新创建档案".to_string());
        }
        if self.position == 0 {
            return Err("Codex 供应商排序位置必须大于零".to_string());
        }
        self.profile.validate()?;
        self.parameters
            .validate_provider_parameters(crate::contracts::AppKind::Codex)
            .map_err(|error| error.to_string())?;
        if let Some(query) = &self.usage_query {
            crate::validate::validate_usage_query(query).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn record(&self, file_hash: String) -> CodexProviderRecord {
        CodexProviderRecord {
            profile: self.profile.clone(),
            position: self.position,
            parameters: self.parameters.clone(),
            notes: self.notes.clone(),
            website_url: self.website_url.clone(),
            usage_query: self.usage_query.clone(),
            file_hash,
        }
    }

    pub fn model_catalog_json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string_pretty(&codex_model_catalog_document(&self.profile.catalog))
            .map_err(|_| "Codex 模型目录序列化失败".to_string())
    }

    pub fn client_projection(&self) -> ProviderFile {
        let default = self
            .profile
            .catalog
            .iter()
            .find(|entry| entry.id == self.profile.default_model)
            .expect("validated Codex profile always contains its default model");
        ProviderFile {
            authentication: self.profile.authentication,
            id: self.profile.id.clone(),
            name: self.profile.name.clone(),
            position: self.position,
            route_mode: RouteMode::Custom,
            api_key: self.profile.api_key.clone(),
            upstream_protocol: Some(self.profile.upstream.protocol()),
            responses_options: (self.profile.upstream == CodexUpstream::Responses).then_some(
                ResponsesOptions {
                    request_mode: self.profile.request_mode,
                },
            ),
            max_output_tokens: ExplicitMaxOutputTokens::from(
                (self.profile.upstream == CodexUpstream::AnthropicMessages)
                    .then_some(default.max_output_tokens),
            ),
            base_url: Some(self.profile.endpoint.0.clone()),
            connection: self.profile.connection.clone(),
            model: Some(self.profile.default_model.clone()),
            model_options: Some(ModelOptions::Codex(CodexModelSettings {
                context_window: Some(default.context_window),
            })),
            parameters: self.parameters.clone(),
            claude_fragment: Default::default(),
            notes: self.notes.clone(),
            website_url: self.website_url.clone(),
            display: None,
            usage_query: self.usage_query.clone(),
            official_quota_refresh_interval_minutes: None,
        }
    }
}
