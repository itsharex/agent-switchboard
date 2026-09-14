//! Stable client model roles for an already-running Claude process.

use crate::contracts::{ClaudeModelSettings, ConfigValue, SwitchPlan};

pub(super) fn model_value(
    plan: &SwitchPlan,
    key: &str,
    settings: Option<&ClaudeModelSettings>,
) -> Option<Option<ConfigValue>> {
    let has_default = plan.profile.model.is_some();
    let (role, one_m) = match key {
        "model" if plan.profile.model.is_some() => (
            "asb-claude-primary",
            settings.is_some_and(|s| s.primary_one_m),
        ),
        "env.ANTHROPIC_DEFAULT_HAIKU_MODEL"
            if has_default || settings.is_some_and(|s| s.haiku_model.is_some()) =>
        {
            ("asb-claude-haiku", settings.is_some_and(|s| s.haiku_one_m))
        }
        "env.ANTHROPIC_DEFAULT_SONNET_MODEL"
            if has_default || settings.is_some_and(|s| s.sonnet_model.is_some()) =>
        {
            (
                "asb-claude-sonnet",
                settings.is_some_and(|s| s.sonnet_one_m),
            )
        }
        "env.ANTHROPIC_DEFAULT_OPUS_MODEL"
            if has_default || settings.is_some_and(|s| s.opus_model.is_some()) =>
        {
            ("asb-claude-opus", settings.is_some_and(|s| s.opus_one_m))
        }
        "env.ANTHROPIC_DEFAULT_FABLE_MODEL"
            if has_default || settings.is_some_and(|s| s.fable_model.is_some()) =>
        {
            ("asb-claude-fable", settings.is_some_and(|s| s.fable_one_m))
        }
        "env.CLAUDE_CODE_SUBAGENT_MODEL"
            if settings.is_some_and(|s| s.subagent_model.is_some()) =>
        {
            (
                "asb-claude-subagent",
                settings.is_some_and(|s| s.subagent_one_m),
            )
        }
        _ => return None,
    };
    Some(Some(ConfigValue::Str(crate::claude_model::render_model(
        role, one_m,
    ))))
}
