//! Wire effort dialects for the model families supported by the Claude bridge.

use super::*;

pub(super) fn insert_openai_reasoning(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
    protocol: UpstreamProtocol,
) {
    let Some(effort) = request.reasoning_effort else {
        return;
    };
    let normalized = request.model.to_ascii_lowercase();
    let model = normalized.rsplit('/').next().unwrap_or(&normalized);
    let effort = if supports_openai_effort(model) {
        match effort {
            ReasoningEffort::Max => "xhigh",
            _ => effort_name(effort),
        }
    } else if protocol == UpstreamProtocol::ChatCompletions
        && (model == "kimi-k3" || model.starts_with("kimi-k3-"))
    {
        // Preserve the existing Kimi low/high/max dialect. Its middle tier
        // shares high; OpenAI's maximum spelling must not leak to this path.
        match effort {
            ReasoningEffort::Medium => "high",
            _ => effort_name(effort),
        }
    } else {
        // As in CC Switch, do not inject effort into unrelated model APIs.
        return;
    };
    match protocol {
        UpstreamProtocol::ChatCompletions => {
            root.insert(
                "reasoning_effort".to_string(),
                Value::String(effort.to_string()),
            );
        }
        UpstreamProtocol::Responses => {
            root.insert("reasoning".to_string(), json!({ "effort": effort }));
        }
        UpstreamProtocol::AnthropicMessages | UpstreamProtocol::GeminiGenerateContent => {}
    }
}

fn supports_openai_effort(model: &str) -> bool {
    (model.starts_with('o') && model.as_bytes().get(1).is_some_and(u8::is_ascii_digit))
        || model
            .strip_prefix("gpt-")
            .and_then(|version| version.chars().next())
            .is_some_and(|major| major.is_ascii_digit() && major >= '5')
        || model == "grok-4.5"
        || model.starts_with("grok-4.5-")
        || model.starts_with("grok-build-")
}

pub(super) fn effort_name(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
        ReasoningEffort::Max => "max",
    }
}
