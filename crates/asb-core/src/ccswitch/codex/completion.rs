use crate::ccswitch::row::{CodexCatalogSeed, CodexImportSeed};
use crate::contracts::{
    default_model_limits, CodexCapabilities, CodexCatalogEntry, CodexProviderDraft,
    CodexReasoningLevel, CODEX_REASONING_LADDER, DEFAULT_CODEX_CAPABILITIES,
};

impl CodexImportSeed {
    /// The one completion owner behind one-click Codex import: turns the
    /// source facts into a strict `CodexProviderDraft` with the declared
    /// default capabilities, catalog rows narrowed by the source's explicit
    /// facts, and materialized model limits. The result goes through the same
    /// strict validation as an editor save.
    pub fn completion_draft(&self) -> Result<CodexProviderDraft, String> {
        if self.api_key.trim().is_empty() {
            return Err("来源未提供可用 API 密钥，无法导入".to_string());
        }
        let mut capabilities = DEFAULT_CODEX_CAPABILITIES;
        capabilities.chat_reasoning = self.chat_reasoning.clone();
        let catalog = self
            .catalog
            .iter()
            .map(|seed| completed_catalog_entry(seed, &capabilities))
            .collect::<Vec<_>>();
        Ok(CodexProviderDraft {
            name: self.name.trim().to_string(),
            endpoint: self.endpoint.clone(),
            api_key: self.api_key.trim().to_string(),
            authentication: self.authentication,
            connection: self.connection.clone(),
            upstream: self.upstream,
            request_mode: self.request_mode,
            default_model: self.default_model.trim().to_string(),
            catalog,
            model_routes: Vec::new(),
            capabilities,
            parameters: self.parameters.clone(),
            notes: self.notes.clone(),
            website_url: self.website_url.clone(),
            usage_query: self.usage_query.clone(),
        })
    }
}

/// Materializes one catalog row: explicit source facts win, then the
/// officially published limits, then the generic positive defaults. Flags
/// follow the declared capabilities; reasoning levels narrow to the source's
/// explicit ladder or fall back to the full ladder.
fn completed_catalog_entry(
    seed: &CodexCatalogSeed,
    capabilities: &CodexCapabilities,
) -> CodexCatalogEntry {
    let levels = match &seed.reasoning_levels {
        Some(levels) if !levels.is_empty() => {
            let mut narrowed = Vec::with_capacity(levels.len());
            for level in levels {
                if !narrowed.contains(level) {
                    narrowed.push(*level);
                }
            }
            narrowed
        }
        _ => CODEX_REASONING_LADDER.to_vec(),
    };
    let default_level = seed
        .default_reasoning_level
        .filter(|level| levels.contains(level))
        .unwrap_or_else(|| {
            levels
                .iter()
                .find(|level| **level == CodexReasoningLevel::Medium)
                .copied()
                .unwrap_or(*levels.last().expect("ladder is never empty"))
        });
    let (context_window, max_output_tokens) = match seed.context_window {
        Some(value) if value > 0 => (value, default_model_limits(&seed.model).1),
        _ => default_model_limits(&seed.model),
    };
    CodexCatalogEntry {
        id: seed.model.clone(),
        context_window,
        max_output_tokens,
        function_tools: capabilities.function_tools,
        custom_tools: capabilities.custom_tools,
        tool_search: capabilities.tool_search,
        reasoning: capabilities.reasoning,
        default_reasoning_level: default_level,
        supported_reasoning_levels: levels,
        images: seed.images.unwrap_or(false),
        compact: capabilities.compact,
        display_name: seed.display_name.clone(),
        description: seed.description.clone(),
        base_instructions: seed.base_instructions.clone(),
        supports_parallel_tool_calls: seed.supports_parallel_tool_calls,
    }
}
