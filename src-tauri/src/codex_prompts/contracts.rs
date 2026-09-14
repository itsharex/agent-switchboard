use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PromptDraft {
    pub name: String,
    pub description: Option<String>,
    pub content: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PromptPreset {
    pub id: String,
    pub draft: PromptDraft,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PromptsFile {
    pub version: u8,
    pub active_id: Option<String>,
    pub presets: Vec<PromptPreset>,
}
impl Default for PromptsFile {
    fn default() -> Self {
        Self {
            version: 1,
            active_id: None,
            presets: Vec::new(),
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptsView {
    pub revision: String,
    pub active_id: Option<String>,
    pub presets: Vec<PromptPreset>,
    pub live: asb_core::GlobalPromptDocument,
    pub pending_recovery: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PromptActivation {
    pub preset_id: Option<String>,
    pub revision: String,
    pub live_hash: String,
    pub rendered_hash: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptPreview {
    pub plan: PromptActivation,
    pub before: String,
    pub after: String,
    pub backfills_active: bool,
}
