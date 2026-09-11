//! Explicit Responses-to-Anthropic reasoning dialect rendering.
//!
//! Codex selects a reasoning level through `reasoning.effort`. Anthropic
//! Messages expresses the same intent through `output_config.effort`, and an
//! explicit "do not reason" request through `thinking.type = "disabled"`.
//! The dialect lives here so the cross-protocol renderer never guesses a
//! level, and so a level with no Anthropic equivalent fails loudly.

use super::TransformError;
use asb_core::contracts::UpstreamProtocol;
use serde_json::{json, Value};

pub(super) enum Directive {
    /// The client explicitly asked for no reasoning on this request.
    Disabled,
    /// The selected Anthropic `output_config.effort` level.
    Effort(&'static str),
}

pub(super) fn extract(
    from: UpstreamProtocol,
    to: UpstreamProtocol,
    value: &mut Value,
) -> Result<Option<Directive>, TransformError> {
    if from != UpstreamProtocol::Responses || to != UpstreamProtocol::AnthropicMessages {
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
    Ok(Some(if disabled(&effort) {
        Directive::Disabled
    } else {
        Directive::Effort(effort_level(&effort)?)
    }))
}

pub(super) fn render(
    value: &mut Value,
    directive: Option<Directive>,
) -> Result<(), TransformError> {
    let Some(directive) = directive else {
        return Ok(());
    };
    let root = value
        .as_object_mut()
        .ok_or_else(|| TransformError("转换后的 Anthropic 请求必须是对象".to_string()))?;
    match directive {
        Directive::Disabled => {
            root.insert("thinking".to_string(), json!({ "type": "disabled" }));
        }
        Directive::Effort(effort) => {
            root.insert("output_config".to_string(), json!({ "effort": effort }));
        }
    }
    Ok(())
}

fn disabled(effort: &str) -> bool {
    matches!(effort, "none" | "off" | "disabled")
}

fn effort_level(effort: &str) -> Result<&'static str, TransformError> {
    match effort {
        "minimal" | "low" => Ok("low"),
        "medium" => Ok("medium"),
        "high" => Ok("high"),
        "xhigh" | "max" | "ultra" => Ok("max"),
        other => Err(TransformError(format!(
            "reasoning.effort={other} 不受所选 Anthropic 上游支持"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_value(value: Value) -> Result<Option<Directive>, TransformError> {
        let mut value = value;
        extract(
            UpstreamProtocol::Responses,
            UpstreamProtocol::AnthropicMessages,
            &mut value,
        )
    }

    #[test]
    fn codex_levels_map_onto_the_anthropic_effort_dialect() {
        for (effort, expected) in [
            ("minimal", "low"),
            ("low", "low"),
            ("medium", "medium"),
            ("high", "high"),
            ("xhigh", "max"),
            ("max", "max"),
            ("ultra", "max"),
        ] {
            let directive = extract_value(json!({"reasoning": {"effort": effort}}))
                .expect("recognized effort")
                .expect("directive");
            let mut rendered = json!({"model": "m", "messages": []});
            render(&mut rendered, Some(directive)).expect("render effort");
            assert_eq!(rendered["output_config"]["effort"], expected, "{effort}");
            assert!(rendered.get("thinking").is_none());
        }
    }

    #[test]
    fn explicit_none_disables_anthropic_thinking_and_drops_the_effort_field() {
        let mut value = json!({"reasoning": {"effort": "none", "summary": "auto"}});
        let directive = extract(
            UpstreamProtocol::Responses,
            UpstreamProtocol::AnthropicMessages,
            &mut value,
        )
        .expect("recognized effort")
        .expect("directive");
        assert_eq!(value["reasoning"], json!({"summary": "auto"}));
        let mut rendered = json!({"model": "m"});
        render(&mut rendered, Some(directive)).expect("render disabled thinking");
        assert_eq!(rendered["thinking"]["type"], "disabled");
        assert!(rendered.get("output_config").is_none());
    }

    #[test]
    fn an_unmapped_level_is_refused_instead_of_guessed() {
        let error = extract_value(json!({"reasoning": {"effort": "turbo"}}))
            .err()
            .expect("unmapped level must fail");
        assert!(error.0.contains("reasoning.effort=turbo"), "{}", error.0);
    }

    #[test]
    fn other_protocol_pairs_keep_the_field_untouched() {
        let mut value = json!({"reasoning": {"effort": "high"}});
        assert!(extract(
            UpstreamProtocol::Responses,
            UpstreamProtocol::ChatCompletions,
            &mut value,
        )
        .expect("chat keeps its own dialect")
        .is_none());
        assert_eq!(value["reasoning"]["effort"], "high");
    }

    #[test]
    fn a_summary_only_directive_is_not_a_reasoning_level() {
        assert!(extract_value(json!({"reasoning": {"summary": "auto"}}))
            .expect("summary alone is not a level")
            .is_none());
    }
}
