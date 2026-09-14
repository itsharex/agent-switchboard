//! Offline Claude preset preparation. Preparation never activates a provider or writes client files.

mod source;
#[cfg(test)]
mod tests;
use crate::{
    contracts::{ProviderDraft, UpstreamProtocol},
    map_row, CcSwitchProviderDraft,
};
use serde::Serialize;
pub use source::TemplateValue as ClaudePresetVariable;
use std::collections::BTreeMap;

pub const SOURCE_REVISION: &str = "d695a2d77fd9081eafd3e9eedcbf2a97b3410928";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudePresetSummary {
    pub id: String,
    pub name: String,
    pub website_url: String,
    pub api_key_url: Option<String>,
    pub category: String,
    pub authentication: String,
    pub upstream_protocol: Option<UpstreamProtocol>,
    pub endpoint_candidates: Vec<String>,
    pub variables: BTreeMap<String, ClaudePresetVariable>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudePresetPreparation {
    pub draft: ProviderDraft,
    pub warnings: Vec<String>,
}

pub fn list() -> Result<Vec<ClaudePresetSummary>, String> {
    Ok(source::catalog()?
        .presets
        .into_iter()
        .map(|preset| {
            let unavailable_reason = preset.unsupported_reason();
            let native_credentials = preset.template_values.contains_key("AWS_SECRET_ACCESS_KEY");
            let protocol = match preset.api_format.as_deref().unwrap_or("anthropic") {
                "anthropic" => Some(UpstreamProtocol::AnthropicMessages),
                "openai_chat" => Some(UpstreamProtocol::ChatCompletions),
                "openai_responses" => Some(UpstreamProtocol::Responses),
                "gemini_native" => Some(UpstreamProtocol::GeminiGenerateContent),
                _ => None,
            };
            ClaudePresetSummary {
                id: preset.id,
                name: if preset.is_official {
                    "Claude 官方登录".into()
                } else {
                    preset.name
                },
                website_url: preset.website_url,
                api_key_url: preset.api_key_url,
                category: preset.category.unwrap_or("custom".into()),
                authentication: if preset.is_official {
                    "official".into()
                } else if native_credentials {
                    "native_sdk".into()
                } else {
                    preset.provider_type.unwrap_or("api_key".into())
                },
                upstream_protocol: protocol,
                endpoint_candidates: preset.endpoint_candidates,
                variables: preset.template_values,
                unavailable_reason,
            }
        })
        .collect())
}

pub fn prepare(
    id: &str,
    api_key: &str,
    variables: &BTreeMap<String, String>,
    account_id: Option<&str>,
) -> Result<ClaudePresetPreparation, String> {
    let preset = source::catalog()?
        .presets
        .into_iter()
        .find(|preset| preset.id == id)
        .ok_or("Claude 预设不存在")?;
    let proposal =
        map_row(&preset.row(api_key, variables, account_id)?).map_err(|skip| skip.reason)?;
    let CcSwitchProviderDraft::Claude(draft) = proposal.draft else {
        return Err("Claude 预设生成了不匹配的客户端档案".into());
    };
    draft.validate().map_err(|error| error.to_string())?;
    Ok(ClaudePresetPreparation {
        draft,
        warnings: proposal.warnings,
    })
}
