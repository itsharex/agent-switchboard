use super::{ResponsesRequestMode, SettingsValues, UpstreamProtocol, UsageQuery};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const CODEX_PROVIDER_SCHEMA_VERSION: u8 = 1;

/// A validated Codex upstream API root. The value is never an operation URL;
/// operation paths are appended only by the gateway endpoint resolver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CodexEndpoint(pub String);

/// The only third-party protocols a Codex profile may target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CodexUpstream {
    Responses,
    ChatCompletions,
    AnthropicMessages,
}

impl CodexUpstream {
    pub const fn protocol(self) -> UpstreamProtocol {
        match self {
            Self::Responses => UpstreamProtocol::Responses,
            Self::ChatCompletions => UpstreamProtocol::ChatCompletions,
            Self::AnthropicMessages => UpstreamProtocol::AnthropicMessages,
        }
    }
}

/// The operations which may be exposed on the fixed local Codex endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CodexOperation {
    Responses,
    Compact,
    Models,
    ChatCompletions,
    AlphaSearch,
    ImageGeneration,
    ImageEdit,
}

/// The declared rendering of a Codex Responses reasoning request when a
/// provider exposes only Chat Completions. This is deliberately profile data:
/// provider names and endpoint hosts are never used to infer a dialect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CodexChatReasoning {
    Unsupported,
    Configured {
        thinking_parameter: CodexChatThinkingParameter,
        effort_parameter: CodexChatEffortParameter,
        effort_mode: CodexChatEffortMode,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CodexChatThinkingParameter {
    None,
    Thinking,
    EnableThinking,
    ReasoningSplit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CodexChatEffortParameter {
    None,
    ReasoningEffort,
    ReasoningObject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CodexChatEffortMode {
    Passthrough,
    LowHigh,
    DeepSeek,
    OpenRouter,
}

/// One Codex-visible reasoning level declared for a specific catalog model.
/// The provider file owns these levels; the generated model catalog merely
/// projects them for the local Codex client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodexReasoningLevel {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
}

impl CodexReasoningLevel {
    pub const fn description(self) -> &'static str {
        match self {
            Self::None => "Disable Thinking",
            Self::Minimal => "Minimal Thinking",
            Self::Low => "Low Thinking",
            Self::Medium => "Medium Thinking",
            Self::High => "High Thinking",
            Self::Xhigh => "Extra High Thinking",
            Self::Max => "Max Thinking",
            Self::Ultra => "Ultra Thinking",
        }
    }
}

/// Capabilities declared by a provider. None is inferred from its name or a
/// response from `/models`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexCapabilities {
    pub responses: bool,
    pub compact: bool,
    pub models: bool,
    pub chat_completions: bool,
    pub alpha_search: bool,
    pub image_generation: bool,
    pub image_edit: bool,
    pub function_tools: bool,
    pub custom_tools: bool,
    pub tool_search: bool,
    pub reasoning: bool,
    pub chat_reasoning: CodexChatReasoning,
}

/// The full reasoning ladder a generated catalog row declares when neither
/// the source nor the user narrows it. The editor mirrors this order in
/// `REASONING_LEVELS` (src/components/codex-provider-editor/draft.ts).
pub const CODEX_REASONING_LADDER: [CodexReasoningLevel; 8] = [
    CodexReasoningLevel::None,
    CodexReasoningLevel::Minimal,
    CodexReasoningLevel::Low,
    CodexReasoningLevel::Medium,
    CodexReasoningLevel::High,
    CodexReasoningLevel::Xhigh,
    CodexReasoningLevel::Max,
    CodexReasoningLevel::Ultra,
];

impl CodexCapabilities {
    pub const fn supports_operation(&self, operation: CodexOperation) -> bool {
        match operation {
            CodexOperation::Responses => self.responses,
            CodexOperation::Compact => self.compact,
            CodexOperation::Models => self.models,
            CodexOperation::ChatCompletions => self.chat_completions,
            CodexOperation::AlphaSearch => self.alpha_search,
            CodexOperation::ImageGeneration => self.image_generation,
            CodexOperation::ImageEdit => self.image_edit,
        }
    }
}

/// The capability declaration a generated provider starts from: the full
/// Codex-compatible surface except the chat-completions-only extras. The
/// editor mirrors this constant as its interactive default
/// (`DEFAULT_CODEX_CAPABILITIES` in src/components/codex-provider-editor/draft.ts);
/// this one owns what a one-click import persists.
pub const DEFAULT_CODEX_CAPABILITIES: CodexCapabilities = CodexCapabilities {
    responses: true,
    compact: true,
    models: true,
    chat_completions: false,
    alpha_search: false,
    image_generation: false,
    image_edit: false,
    function_tools: true,
    custom_tools: true,
    tool_search: true,
    reasoning: true,
    chat_reasoning: CodexChatReasoning::Unsupported,
};

/// Official published model limits, cited per official model id (OpenAI model
/// pages, 2026-09). A source-stated fact always wins; the editor mirrors this
/// table as `OFFICIAL_MODEL_LIMITS` in src/components/codex-provider-editor/draft.ts.
pub fn official_model_limits(model_id: &str) -> Option<(u64, u64)> {
    match model_id {
        "gpt-6-astra" | "gpt-5.6-sol" | "gpt-5.6-terra" | "gpt-5.6-luna" => {
            Some((1_050_000, 128_000))
        }
        _ => None,
    }
}

/// The single owner of "leave empty" limit resolution: official published
/// limits for a known official id, the generic positive defaults otherwise.
pub fn default_model_limits(model_id: &str) -> (u64, u64) {
    official_model_limits(model_id).unwrap_or((128_000, 8_192))
}

/// A client-visible model. The model directory is application-owned input;
/// it is rendered to Codex's `model_catalog_json` projection during a switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexCatalogEntry {
    pub id: String,
    pub context_window: u64,
    pub max_output_tokens: u64,
    pub function_tools: bool,
    pub custom_tools: bool,
    pub tool_search: bool,
    pub reasoning: bool,
    pub default_reasoning_level: CodexReasoningLevel,
    pub supported_reasoning_levels: Vec<CodexReasoningLevel>,
    pub images: bool,
    pub compact: bool,
}

impl CodexCatalogEntry {
    /// Renders one entry in Codex's external model-catalog schema. Provider
    /// files own the source facts; both the on-disk projection and `/v1/models`
    /// use this exact representation.
    pub fn model_catalog_entry_json(&self, priority: usize) -> Value {
        json!({
            "slug": self.id,
            "display_name": self.id,
            "description": self.id,
            "base_instructions": "You are Codex, a coding agent. You and the user share the same workspace and collaborate to achieve the user's goals.",
            "default_reasoning_level": self.default_reasoning_level,
            "supported_reasoning_levels": self.supported_reasoning_levels.iter().map(|level| json!({
                "effort": level,
                "description": level.description(),
            })).collect::<Vec<_>>(),
            "shell_type": "shell_command",
            "visibility": "list",
            "supported_in_api": true,
            "priority": 1000 + priority,
            "supports_reasoning_summaries": true,
            "default_reasoning_summary": "none",
            "support_verbosity": false,
            "truncation_policy": {"mode": "bytes", "limit": 10_000},
            "supports_parallel_tool_calls": self.function_tools,
            "supports_image_detail_original": false,
            "context_window": self.context_window,
            "max_output_tokens": self.max_output_tokens,
            "max_context_window": self.context_window,
            "effective_context_window_percent": 95,
            "experimental_supported_tools": [],
            "input_modalities": if self.images { vec!["text", "image"] } else { vec!["text"] },
            "supports_search_tool": self.tool_search,
            "supports_function_tools": self.function_tools,
            "supports_custom_tools": self.custom_tools,
            "supports_tool_search": self.tool_search,
            "supports_reasoning": self.reasoning,
            "supports_compact": self.compact,
        })
    }
}

/// The one client-facing Codex model directory projection. A provider file
/// validates its catalog before writing this document; the active route
/// snapshot is built from that same validated catalog.
pub fn codex_model_catalog_document(catalog: &[CodexCatalogEntry]) -> Value {
    json!({
        "models": catalog
            .iter()
            .enumerate()
            .map(|(priority, entry)| entry.model_catalog_entry_json(priority))
            .collect::<Vec<_>>(),
    })
}

/// One explicit client-model to upstream-model binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexModelRoute {
    pub client_model: String,
    pub upstream_model: String,
}

/// The sole persistent third-party Codex profile. Official login is a separate
/// generic-store record, so it is never encoded here: a third-party file
/// always means a custom route.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexProviderProfile {
    pub id: String,
    pub name: String,
    pub endpoint: CodexEndpoint,
    pub api_key: String,
    pub upstream: CodexUpstream,
    /// The native Responses request projection is an explicit Codex routing
    /// fact. It is never inferred from the selected upstream.
    pub request_mode: ResponsesRequestMode,
    pub default_model: String,
    pub catalog: Vec<CodexCatalogEntry>,
    pub model_routes: Vec<CodexModelRoute>,
    pub capabilities: CodexCapabilities,
}

/// Editable Codex third-party provider data. The application assigns the
/// stable UUID on creation; this type deliberately has no route-mode or
/// official-login fields because an official Codex route is not a profile.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexProviderDraft {
    pub name: String,
    pub endpoint: CodexEndpoint,
    pub api_key: String,
    pub upstream: CodexUpstream,
    pub request_mode: ResponsesRequestMode,
    pub default_model: String,
    pub catalog: Vec<CodexCatalogEntry>,
    pub model_routes: Vec<CodexModelRoute>,
    pub capabilities: CodexCapabilities,
    pub parameters: SettingsValues,
    pub notes: Option<String>,
    pub website_url: Option<String>,
    pub usage_query: Option<UsageQuery>,
}

impl std::fmt::Debug for CodexProviderDraft {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CodexProviderDraft")
            .field("name", &self.name)
            .field("endpoint", &self.endpoint)
            .field("api_key", &crate::redact::REDACTED)
            .field("upstream", &self.upstream)
            .field("default_model", &self.default_model)
            .field("catalog", &self.catalog)
            .field("model_routes", &self.model_routes)
            .field("capabilities", &self.capabilities)
            .finish_non_exhaustive()
    }
}

/// The only on-disk Codex third-party provider format. Position and local UI
/// metadata are storage concerns; the embedded profile owns every upstream
/// routing, model, and capability fact.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexProviderFile {
    pub schema_version: u8,
    pub position: u64,
    pub profile: CodexProviderProfile,
    pub parameters: SettingsValues,
    pub notes: Option<String>,
    pub website_url: Option<String>,
    pub usage_query: Option<UsageQuery>,
}

impl std::fmt::Debug for CodexProviderFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CodexProviderFile")
            .field("schema_version", &self.schema_version)
            .field("profile_id", &self.profile.id)
            .field("profile_name", &self.profile.name)
            .field("position", &self.position)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexProviderRecord {
    pub profile: CodexProviderProfile,
    pub parameters: SettingsValues,
    pub notes: Option<String>,
    pub website_url: Option<String>,
    pub usage_query: Option<UsageQuery>,
    pub file_hash: String,
}

impl std::fmt::Debug for CodexProviderProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CodexProviderProfile")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("endpoint", &self.endpoint)
            .field("api_key", &crate::redact::REDACTED)
            .field("upstream", &self.upstream)
            .field("default_model", &self.default_model)
            .field("catalog", &self.catalog)
            .field("model_routes", &self.model_routes)
            .field("capabilities", &self.capabilities)
            .finish()
    }
}

impl CodexProviderProfile {
    pub fn validate(&self) -> Result<(), String> {
        validate::profile(self)
    }

    /// Resolves the upstream model once at request acceptance. Explicit
    /// mappings win; native Responses retains an unmapped requested model;
    /// protocol bridges use the profile default.
    pub fn resolve_model(&self, requested: &str) -> Result<String, String> {
        required(requested, "请求模型")?;
        if let Some(route) = self
            .model_routes
            .iter()
            .find(|route| route.client_model == requested)
        {
            return Ok(route.upstream_model.clone());
        }
        if self.upstream == CodexUpstream::Responses {
            return Ok(requested.to_string());
        }
        Ok(self.default_model.clone())
    }
}

pub(super) fn required(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field}不能为空"))
    } else {
        Ok(())
    }
}

mod file;
mod validate;

/// An immutable, credential-bearing snapshot created from a validated profile
/// at the instant a request is accepted. It is never persisted in gateway
/// state and therefore cannot be reconstructed from only a capability token.
#[derive(Clone)]
pub struct CodexRouteSnapshot {
    pub revision: String,
    pub provider_id: String,
    pub endpoint: CodexEndpoint,
    pub api_key: String,
    pub upstream: CodexUpstream,
    pub request_mode: ResponsesRequestMode,
    pub default_model: String,
    pub catalog: Vec<CodexCatalogEntry>,
    pub model_routes: Vec<CodexModelRoute>,
    pub capabilities: CodexCapabilities,
}

impl CodexRouteSnapshot {
    pub fn from_profile(profile: &CodexProviderProfile, revision: String) -> Result<Self, String> {
        profile.validate()?;
        required(&revision, "路由修订")?;
        Ok(Self {
            revision,
            provider_id: profile.id.clone(),
            endpoint: profile.endpoint.clone(),
            api_key: profile.api_key.clone(),
            upstream: profile.upstream,
            request_mode: profile.request_mode,
            default_model: profile.default_model.clone(),
            catalog: profile.catalog.clone(),
            model_routes: profile.model_routes.clone(),
            capabilities: profile.capabilities.clone(),
        })
    }

    pub fn resolve_model(&self, requested: &str) -> Result<String, String> {
        required(requested, "请求模型")?;
        if !self.catalog.iter().any(|entry| entry.id == requested) {
            return Err(format!("请求模型不在已激活 Codex 目录中：{requested}"));
        }
        if let Some(route) = self
            .model_routes
            .iter()
            .find(|route| route.client_model == requested)
        {
            return Ok(route.upstream_model.clone());
        }
        if self.upstream == CodexUpstream::Responses {
            return Ok(requested.to_string());
        }
        Ok(self.default_model.clone())
    }
}

#[cfg(test)]
#[path = "codex/tests.rs"]
mod tests;
