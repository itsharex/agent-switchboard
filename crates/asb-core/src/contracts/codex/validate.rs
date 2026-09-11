use std::collections::HashSet;

use super::{
    required, CodexCapabilities, CodexCatalogEntry, CodexChatReasoning, CodexProviderProfile,
    CodexReasoningLevel, CodexUpstream,
};

pub(super) fn profile(profile: &CodexProviderProfile) -> Result<(), String> {
    validate_identity(profile)?;
    validate_capabilities(&profile.capabilities, profile.upstream)?;
    validate_catalog(profile)?;
    validate_model_routes(profile)
}

fn validate_identity(profile: &CodexProviderProfile) -> Result<(), String> {
    required(&profile.id, "供应商标识")?;
    required(&profile.name, "供应商名称")?;
    required(&profile.api_key, "API 密钥")?;
    crate::endpoint::validate_base_url(&profile.endpoint.0, profile.upstream.protocol())?;
    if matches!(
        profile.upstream,
        CodexUpstream::Responses | CodexUpstream::ChatCompletions
    ) && url::Url::parse(&profile.endpoint.0)
        .map_err(|_| "Codex 服务地址无效".to_string())?
        .path()
        .trim_matches('/')
        .is_empty()
    {
        return Err("OpenAI 兼容 Codex 服务地址必须包含显式 API 根路径".to_string());
    }
    Ok(())
}

fn validate_capabilities(
    capabilities: &CodexCapabilities,
    upstream: CodexUpstream,
) -> Result<(), String> {
    if !capabilities.responses {
        return Err("Codex 能力必须声明 responses".to_string());
    }
    if !capabilities.reasoning
        && !matches!(capabilities.chat_reasoning, CodexChatReasoning::Unsupported)
    {
        return Err("未启用 reasoning 时 chatReasoning 必须为 unsupported".to_string());
    }
    if upstream != CodexUpstream::ChatCompletions
        && !matches!(capabilities.chat_reasoning, CodexChatReasoning::Unsupported)
    {
        return Err("非 Chat Completions 上游不能声明 chatReasoning".to_string());
    }
    Ok(())
}

fn validate_catalog(profile: &CodexProviderProfile) -> Result<(), String> {
    if profile.catalog.is_empty() {
        return Err("Codex 模型目录不能为空".to_string());
    }
    let mut ids = HashSet::new();
    for entry in &profile.catalog {
        validate_catalog_entry(entry, &profile.capabilities)?;
        if !ids.insert(entry.id.as_str()) {
            return Err(format!("Codex 模型目录含有重复模型：{}", entry.id));
        }
    }
    required(&profile.default_model, "Codex 默认模型")?;
    if !ids.contains(profile.default_model.as_str()) {
        return Err("Codex 默认模型必须存在于模型目录".to_string());
    }
    Ok(())
}

fn validate_catalog_entry(
    entry: &CodexCatalogEntry,
    capabilities: &CodexCapabilities,
) -> Result<(), String> {
    required(&entry.id, "目录模型标识")?;
    if entry.context_window == 0 || entry.max_output_tokens == 0 {
        return Err("Codex 模型目录的上下文窗口和输出上限必须大于零".to_string());
    }
    if entry.function_tools && !capabilities.function_tools {
        return Err(format!("模型 functionTools 超出供应商能力：{}", entry.id));
    }
    if entry.custom_tools && !capabilities.custom_tools {
        return Err(format!("模型 customTools 超出供应商能力：{}", entry.id));
    }
    if entry.tool_search && !capabilities.tool_search {
        return Err(format!("模型 toolSearch 超出供应商能力：{}", entry.id));
    }
    if entry.compact && !capabilities.compact {
        return Err(format!("模型 compact 超出供应商能力：{}", entry.id));
    }
    if entry.reasoning && !capabilities.reasoning {
        return Err(format!("模型 reasoning 超出供应商能力：{}", entry.id));
    }
    validate_reasoning_levels(entry)
}

fn validate_reasoning_levels(entry: &CodexCatalogEntry) -> Result<(), String> {
    if entry.supported_reasoning_levels.is_empty() {
        return Err(format!("Codex 模型目录未声明推理档位：{}", entry.id));
    }
    let levels = entry
        .supported_reasoning_levels
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    if levels.len() != entry.supported_reasoning_levels.len() {
        return Err(format!("Codex 模型目录含有重复推理档位：{}", entry.id));
    }
    if !levels.contains(&entry.default_reasoning_level) {
        return Err(format!("Codex 模型默认推理档位未声明：{}", entry.id));
    }
    if !entry.reasoning
        && (entry.default_reasoning_level != CodexReasoningLevel::None
            || levels.len() != 1
            || !levels.contains(&CodexReasoningLevel::None))
    {
        return Err(format!(
            "不支持推理的 Codex 模型只能声明 none 档位：{}",
            entry.id
        ));
    }
    Ok(())
}

fn validate_model_routes(profile: &CodexProviderProfile) -> Result<(), String> {
    let catalog = profile
        .catalog
        .iter()
        .map(|entry| entry.id.as_str())
        .collect::<HashSet<_>>();
    let mut sources = HashSet::new();
    for route in &profile.model_routes {
        required(&route.client_model, "客户端模型标识")?;
        required(&route.upstream_model, "上游模型标识")?;
        if !catalog.contains(route.client_model.as_str()) {
            return Err(format!(
                "模型映射引用了目录外客户端模型：{}",
                route.client_model
            ));
        }
        if !sources.insert(route.client_model.as_str()) {
            return Err(format!(
                "Codex 模型映射含有重复客户端模型：{}",
                route.client_model
            ));
        }
    }
    Ok(())
}
