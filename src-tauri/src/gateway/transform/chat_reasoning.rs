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
    if thinking_parameter == CodexChatThinkingParameter::None
        && effort_parameter == CodexChatEffortParameter::None
    {
        // Fixed-reasoning presets expose no toggle. Never acknowledge an off
        // request while the upstream would keep thinking.
        if !directive.enabled {
            return error("所选 Chat 上游的固定推理模式不能关闭");
        }
        if map_effort(&directive.effort, CodexChatEffortMode::Passthrough).is_none() {
            return error("Codex 推理档位不受支持");
        }
        return Ok(());
    }
    let root = value
        .as_object_mut()
        .ok_or_else(|| TransformError("转换后的 Chat 请求必须是对象".to_string()))?;
    render_thinking(root, thinking_parameter, directive.enabled);
    if !directive.enabled {
        return render_disabled_effort(root, thinking_parameter, effort_parameter, effort_mode);
    }
    render_effort(root, effort_parameter, effort_mode, &directive.effort)
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

fn render_disabled_effort(
    root: &mut Map<String, Value>,
    thinking: CodexChatThinkingParameter,
    parameter: CodexChatEffortParameter,
    mode: CodexChatEffortMode,
) -> Result<(), TransformError> {
    match parameter {
        CodexChatEffortParameter::ReasoningObject => {
            root.insert("reasoning".to_string(), json!({"effort":"none"}));
        }
        CodexChatEffortParameter::ReasoningEffort
            if thinking == CodexChatThinkingParameter::None =>
        {
            // Restricted effort enums require a declared toggle to express off.
            if !matches!(
                mode,
                CodexChatEffortMode::Passthrough | CodexChatEffortMode::Catalog
            ) {
                return error("所选 Chat effort 方言无法表达关闭推理；请配置明确的思考开关");
            }
            root.insert("reasoning_effort".to_string(), json!("none"));
        }
        _ => {}
    }
    Ok(())
}

fn render_effort(
    root: &mut Map<String, Value>,
    parameter: CodexChatEffortParameter,
    mode: CodexChatEffortMode,
    effort: &str,
) -> Result<(), TransformError> {
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
        CodexChatEffortMode::Passthrough | CodexChatEffortMode::Catalog => match effort {
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
