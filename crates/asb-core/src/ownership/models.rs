use crate::contracts::AppKind;
use crate::ownership::spec::{ModelSpec, SettingOwner};

/// The default model used by a Codex sub-agent when its role and task do not
/// provide one. It belongs to the active provider because its id must resolve
/// against that provider's upstream model catalog.
pub const CODEX_SUBAGENT_MODEL_KEY: &str = "agents.default_subagent_model";

pub(super) const MODEL_SPECS: &[ModelSpec] = &[ModelSpec {
    app: AppKind::Codex,
    key: CODEX_SUBAGENT_MODEL_KEY,
    owner: SettingOwner::Provider,
    label: "默认子 agent 模型",
    group: "子 agent",
}];
