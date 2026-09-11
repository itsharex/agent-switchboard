//! Explicit Responses-to-Chat reasoning dialect rendering.

use super::{error, TransformError};
use asb_core::contracts::{
    CodexChatEffortMode, CodexChatEffortParameter, CodexChatReasoning, CodexChatThinkingParameter,
    UpstreamProtocol,
};
use serde_json::{json, Map, Value};

pub(super) struct Directive {
    enabled: bool,
    effort: String,
    configuration: CodexChatReasoning,
}

pub(super) fn extract(
    from: UpstreamProtocol,
    to: UpstreamProtocol,
    value: &mut Value,
    configuration: Option<&CodexChatReasoning>,
) -> Result<Option<Directive>, TransformError> {
    if from != UpstreamProtocol::Responses || to != UpstreamProtocol::ChatCompletions {
        return Ok(None);
    }
    let root = value
        .as_object_mut()
        .ok_or_else(|| TransformError("Responses 请求必须是 JSON 对象".to_string()))?;
    let Some(reasoning) = root.get_mut("reasoning") else {
        return Ok(None);
    };
    let reasoning = reasoning
        .as_object_mut()
        .ok_or_else(|| TransformError("reasoning 必须是对象".to_string()))?;
    let Some(effort) = reasoning.remove("effort") else {
        return Ok(None);
    };
    let effort = effort
        .as_str()
        .ok_or_else(|| TransformError("reasoning.effort 必须是字符串".to_string()))?
        .to_ascii_lowercase();
    if reasoning.is_empty() {
        root.remove("reasoning");
    }
    let Some(CodexChatReasoning::Configured {
        thinking_parameter,
        effort_parameter,
        effort_mode,
    }) = configuration
    else {
        return error("Chat 上游未声明 Codex reasoning.effort 方言");
    };
    Ok(Some(Directive {
        enabled: !matches!(effort.as_str(), "none" | "off" | "disabled"),
        effort,
        configuration: CodexChatReasoning::Configured {
            thinking_parameter: *thinking_parameter,
            effort_parameter: *effort_parameter,
            effort_mode: *effort_mode,
        },
    }))
}

pub(super) fn render(
    value: &mut Value,
    directive: Option<Directive>,
) -> Result<(), TransformError> {
    let Some(directive) = directive else {
        return Ok(());
    };
    let CodexChatReasoning::Configured {
        thinking_parameter,
        effort_parameter,
        effort_mode,
    } = directive.configuration
    else {
        return error("Chat 上游未声明 Codex reasoning.effort 方言");
    };
    let root = value
        .as_object_mut()
        .ok_or_else(|| TransformError("转换后的 Chat 请求必须是对象".to_string()))?;
    render_thinking(root, thinking_parameter, directive.enabled);
    render_effort(
        root,
        effort_parameter,
        effort_mode,
        &directive.effort,
        directive.enabled,
    )
}

fn render_thinking(
    root: &mut Map<String, Value>,
    parameter: CodexChatThinkingParameter,
    enabled: bool,
) {
    match parameter {
        CodexChatThinkingParameter::None => {}
        CodexChatThinkingParameter::Thinking => {
            root.insert(
                "thinking".to_string(),
                json!({"type": if enabled { "enabled" } else { "disabled" }}),
            );
        }
        CodexChatThinkingParameter::EnableThinking => {
            root.insert("enable_thinking".to_string(), Value::Bool(enabled));
        }
        CodexChatThinkingParameter::ReasoningSplit => {
            root.insert("reasoning_split".to_string(), Value::Bool(enabled));
        }
    }
}

fn render_effort(
    root: &mut Map<String, Value>,
    parameter: CodexChatEffortParameter,
    mode: CodexChatEffortMode,
    effort: &str,
    enabled: bool,
) -> Result<(), TransformError> {
    if !enabled {
        if parameter == CodexChatEffortParameter::ReasoningObject {
            root.insert("reasoning".to_string(), json!({"effort":"none"}));
        }
        return Ok(());
    }
    let Some(effort) = map_effort(effort, mode) else {
        return error(format!("reasoning.effort={effort} 不受所选 Chat 上游支持"));
    };
    match parameter {
        CodexChatEffortParameter::None => {}
        CodexChatEffortParameter::ReasoningEffort => {
            root.insert("reasoning_effort".to_string(), Value::String(effort));
        }
        CodexChatEffortParameter::ReasoningObject => {
            root.insert("reasoning".to_string(), json!({"effort":effort}));
        }
    }
    Ok(())
}

fn map_effort(effort: &str, mode: CodexChatEffortMode) -> Option<String> {
    let mapped = match mode {
        CodexChatEffortMode::Passthrough => match effort {
            "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra" => effort,
            _ => return None,
        },
        CodexChatEffortMode::LowHigh => match effort {
            "minimal" | "low" => "low",
            "medium" | "high" | "xhigh" | "max" | "ultra" => "high",
            _ => return None,
        },
        CodexChatEffortMode::DeepSeek => match effort {
            "minimal" | "low" | "medium" | "high" => "high",
            "xhigh" | "max" | "ultra" => "max",
            _ => return None,
        },
        CodexChatEffortMode::OpenRouter => match effort {
            "minimal" | "low" | "medium" | "high" => effort,
            "xhigh" | "max" | "ultra" => "xhigh",
            _ => return None,
        },
    };
    Some(mapped.to_string())
}
