//! Claude prompt library contracts. These never enter the Codex prompt store or client preferences.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudePromptDraft {
    pub name: String,
    pub description: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudePrompt {
    pub id: String,
    pub draft: ClaudePromptDraft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ActivePrompt {
    pub id: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PromptFile {
    pub version: u32,
    pub prompts: Vec<ClaudePrompt>,
    pub active: Option<ActivePrompt>,
}

impl Default for PromptFile {
    fn default() -> Self {
        Self {
            version: 1,
            prompts: Vec::new(),
            active: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudePromptsView {
    pub file_hash: String,
    pub prompts: Vec<ClaudePrompt>,
    pub active_prompt_id: Option<String>,
    pub live_hash: String,
    pub external_change: bool,
    pub pending_content: bool,
    pub recovery_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudePromptActivation {
    pub prompt_id: Option<String>,
    pub file_hash: String,
    pub live_hash: String,
    pub live_exists: bool,
    pub rendered_hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudePromptPreview {
    pub plan: ClaudePromptActivation,
    pub before: String,
    pub after: String,
}

impl ClaudePromptDraft {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty()
            || self.name != self.name.trim()
            || self.name.chars().count() > 120
            || self.name.chars().any(char::is_control)
        {
            return Err("Claude 提示词名称必须为 1–120 个字符，不能有控制字符或首尾空白".into());
        }
        if self.content.len() > 2 * 1024 * 1024 || self.content.contains('\0') {
            return Err("Claude 提示词内容不能超过 2 MiB 或包含空字符".into());
        }
        if self
            .description
            .as_ref()
            .is_some_and(|value| value.len() > 2048 || value.contains('\0'))
        {
            return Err("Claude 提示词说明不能超过 2048 字节或包含空字符".into());
        }
        Ok(())
    }
}

impl PromptFile {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.prompts.len() > 1000 {
            return Err("Claude 提示词库版本或数量无效".into());
        }
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for prompt in &self.prompts {
            prompt.draft.validate()?;
            if uuid::Uuid::parse_str(&prompt.id).is_err()
                || !ids.insert(&prompt.id)
                || !names.insert(prompt.draft.name.to_lowercase())
            {
                return Err("Claude 提示词 ID 或名称重复，或 ID 格式无效".into());
            }
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| !ids.contains(&active.id) || active.content_hash.len() != 64)
        {
            return Err("Claude 活动提示词引用无效".into());
        }
        Ok(())
    }
}
