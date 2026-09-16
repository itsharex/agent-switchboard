//! Offline Codex presets, adapted to ASB's typed providers rather than live TOML writes.
//! The generated data is pinned; no preset download, activation or credential access occurs.

mod source;
#[cfg(test)]
mod tests;

use crate::ccswitch::{map_row, CcSwitchProviderDraft};
use crate::contracts::{CodexProviderDraft, CodexUpstream};
use serde::Serialize;
use source::{catalog, PresetSource};

pub const SOURCE_REVISION: &str = "d695a2d77fd9081eafd3e9eedcbf2a97b3410928";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CodexPresetAuthentication {
    ApiKey,
    NativeOpenAi,
    ManagedXai,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexPresetSummary {
    pub id: String,
    pub name: String,
    pub category: String,
    pub website_url: String,
    pub api_key_url: Option<String>,
    pub authentication: CodexPresetAuthentication,
    pub upstream: Option<CodexUpstream>,
    pub endpoint_candidates: Vec<String>,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexPresetPreparation {
    pub draft: CodexProviderDraft,
    pub warnings: Vec<String>,
}

pub fn list() -> Result<Vec<CodexPresetSummary>, String> {
    catalog()?
        .presets
        .into_iter()
        .map(|preset| preset.summary())
        .collect()
}

pub fn prepare(id: &str, api_key: &str) -> Result<CodexPresetPreparation, String> {
    let preset = catalog()?
        .presets
        .into_iter()
        .find(|preset| preset.id == id)
        .ok_or("Codex 预设不存在")?;
    match preset.authentication() {
        CodexPresetAuthentication::ApiKey => prepare_source(&preset, api_key),
        // Managed xAI presets carry the placeholder credential; the real
        // token is resolved per request from the logged-in xAI account.
        CodexPresetAuthentication::ManagedXai => {
            prepare_source(&preset, crate::contracts::XAI_OAUTH_PLACEHOLDER)
        }
        CodexPresetAuthentication::NativeOpenAi => {
            Err("该 Codex 预设需要专属登录流程，不能使用 API 密钥创建".into())
        }
    }
}

fn prepare_source(preset: &PresetSource, api_key: &str) -> Result<CodexPresetPreparation, String> {
    let proposal = map_row(&preset.row(api_key)?).map_err(|skip| skip.reason)?;
    let CcSwitchProviderDraft::Codex(seed) = proposal.draft else {
        return Err("Codex API 密钥预设未生成第三方档案".into());
    };
    let draft = seed.completion_draft()?;
    draft
        .clone()
        .into_file("preset-validation".into(), 1)
        .validate()?;
    Ok(CodexPresetPreparation {
        draft,
        warnings: proposal.warnings,
    })
}
