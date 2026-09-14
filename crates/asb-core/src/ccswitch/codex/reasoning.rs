//! Explicit CC metadata is translated at import, never guessed at request time.
use crate::contracts::{
    CodexChatEffortMode as Mode, CodexChatEffortParameter as Effort, CodexChatReasoning,
    CodexChatThinkingParameter as Thinking,
};
use serde_json::Value;

pub(super) fn parse(value: Option<&Value>) -> Result<CodexChatReasoning, String> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(CodexChatReasoning::Unsupported);
    };
    let object = value
        .as_object()
        .ok_or("meta.codexChatReasoning 必须是对象")?;
    let enabled = |name: &str| -> Result<bool, String> {
        object
            .get(name)
            .map(|value| {
                value
                    .as_bool()
                    .ok_or_else(|| format!("meta.codexChatReasoning.{name} 必须是布尔值"))
            })
            .transpose()
            .map(|v| v.unwrap_or(false))
    };
    let text = |name: &str| -> Result<&str, String> {
        object
            .get(name)
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| format!("meta.codexChatReasoning.{name} 必须是字符串"))
            })
            .transpose()
            .map(|v| v.unwrap_or("none"))
    };
    let thinking_parameter = if !enabled("supportsThinking")? {
        Thinking::None
    } else {
        match text("thinkingParam")? {
            "none" => Thinking::None,
            "thinking" => Thinking::Thinking,
            "enable_thinking" => Thinking::EnableThinking,
            "reasoning_split" => Thinking::ReasoningSplit,
            _ => return Err("meta.codexChatReasoning.thinkingParam 不受支持".into()),
        }
    };
    let effort_parameter = if !enabled("supportsEffort")? {
        Effort::None
    } else {
        match text("effortParam")? {
            "none" => Effort::None,
            "reasoning_effort" => Effort::ReasoningEffort,
            "reasoning.effort" => Effort::ReasoningObject,
            _ => return Err("meta.codexChatReasoning.effortParam 不受支持".into()),
        }
    };
    let effort_mode = match text("effortValueMode")? {
        "none" | "passthrough" => Mode::Passthrough,
        "low_high" => Mode::LowHigh,
        "deepseek" => Mode::DeepSeek,
        "openrouter" => Mode::OpenRouter,
        "zen" => Mode::Catalog,
        _ => return Err("meta.codexChatReasoning.effortValueMode 不受支持".into()),
    };
    Ok(CodexChatReasoning::Configured {
        thinking_parameter,
        effort_parameter,
        effort_mode,
    })
}
