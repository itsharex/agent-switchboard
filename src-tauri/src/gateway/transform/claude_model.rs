use asb_core::contracts::ClaudeModelSettings;
use serde_json::Value;

const ONE_M_SUFFIX: &str = "[1m]";

/// Applies Claude Code's provider-owned model tiers before the request enters
/// protocol conversion. The client keeps its configured model spelling, while
/// upstream receives only the selected provider model without the local 1M
/// capability marker.
pub(crate) fn apply_claude_model_mapping(
    body: &[u8],
    primary_model: Option<&str>,
    settings: Option<&ClaudeModelSettings>,
) -> Result<Vec<u8>, super::TransformError> {
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|_| super::TransformError("请求体不是有效 JSON".to_string()))?;
    let Some(original) = value.get("model").and_then(Value::as_str) else {
        return Ok(body.to_vec());
    };
    let normalized = strip_one_m_suffix(original);
    let mapped = map_model(normalized, primary_model, settings);
    let outgoing = strip_one_m_suffix(mapped).to_string();
    if outgoing == original {
        return Ok(body.to_vec());
    }
    value["model"] = Value::String(outgoing);
    serde_json::to_vec(&value)
        .map_err(|_| super::TransformError("无法编码 Claude 模型路由请求".to_string()))
}

fn map_model<'a>(
    original: &'a str,
    primary_model: Option<&'a str>,
    settings: Option<&'a ClaudeModelSettings>,
) -> &'a str {
    if original == "asb-claude-subagent" {
        return settings
            .and_then(|value| value.subagent_model.as_deref())
            .or(primary_model)
            .unwrap_or("claude-sonnet-5");
    }
    if original == "asb-claude-primary" {
        return primary_model.unwrap_or("claude-sonnet-5");
    }
    if let Some(subagent) = settings.and_then(|value| value.subagent_model.as_deref()) {
        if strip_one_m_suffix(subagent) == original {
            return original;
        }
    }
    let mapped = if original.to_ascii_lowercase().contains("fable") {
        settings
            .and_then(|value| value.fable_model.as_deref())
            .or_else(|| settings.and_then(|value| value.opus_model.as_deref()))
    } else if original.to_ascii_lowercase().contains("haiku") {
        settings.and_then(|value| value.haiku_model.as_deref())
    } else if original.to_ascii_lowercase().contains("opus") {
        settings.and_then(|value| value.opus_model.as_deref())
    } else if original.to_ascii_lowercase().contains("sonnet") {
        settings.and_then(|value| value.sonnet_model.as_deref())
    } else {
        None
    };
    mapped.or(primary_model).unwrap_or_else(|| match original {
        "asb-claude-haiku" => "claude-haiku-4-5",
        "asb-claude-sonnet" => "claude-sonnet-5",
        "asb-claude-opus" => "claude-opus-5",
        "asb-claude-fable" => "claude-fable-5",
        _ => original,
    })
}

fn strip_one_m_suffix(model: &str) -> &str {
    let trimmed = model.trim_end();
    if trimmed.len() >= ONE_M_SUFFIX.len()
        && trimmed.as_bytes()[trimmed.len() - ONE_M_SUFFIX.len()..]
            .eq_ignore_ascii_case(ONE_M_SUFFIX.as_bytes())
    {
        return trimmed[..trimmed.len() - ONE_M_SUFFIX.len()].trim_end();
    }
    model
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn settings() -> ClaudeModelSettings {
        ClaudeModelSettings {
            haiku_model: Some("vendor-haiku".into()),
            sonnet_model: Some("vendor-sonnet".into()),
            opus_model: Some("vendor-opus".into()),
            fable_model: None,
            subagent_model: Some("vendor-agent".into()),
            ..ClaudeModelSettings::default()
        }
    }

    fn model(body: Value) -> String {
        let bytes = apply_claude_model_mapping(
            &body.to_string().into_bytes(),
            Some("default"),
            Some(&settings()),
        )
        .expect("model mapping");
        serde_json::from_slice::<Value>(&bytes).expect("mapped JSON")["model"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn maps_claude_tiers_and_fable_falls_back_to_opus() {
        assert_eq!(model(json!({"model":"claude-haiku-4"})), "vendor-haiku");
        assert_eq!(
            model(json!({"model":"claude-sonnet-4[1M]"})),
            "vendor-sonnet"
        );
        assert_eq!(model(json!({"model":"claude-opus-4"})), "vendor-opus");
        assert_eq!(model(json!({"model":"claude-fable-4"})), "vendor-opus");
    }

    #[test]
    fn keeps_configured_subagent_model_and_uses_primary_for_unknown_models() {
        assert_eq!(model(json!({"model":"vendor-agent[1m]"})), "vendor-agent");
        assert_eq!(model(json!({"model":"unknown-model"})), "default");
    }

    #[test]
    fn strips_one_m_without_a_mapping() {
        let body = json!({"model":"vendor-model[1m]", "stream":true});
        let bytes = apply_claude_model_mapping(&body.to_string().into_bytes(), None, None)
            .expect("strip marker");
        let mapped: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(mapped["model"], "vendor-model");
        assert_eq!(mapped["stream"], true);
    }

    #[test]
    fn preserves_model_whitespace_when_no_one_m_suffix_is_present() {
        assert_eq!(strip_one_m_suffix("vendor-model "), "vendor-model ");
    }

    #[test]
    fn unicode_model_identifiers_never_slice_inside_a_codepoint() {
        assert_eq!(strip_one_m_suffix("中文模型"), "中文模型");
        assert_eq!(strip_one_m_suffix("中文模型[1M]"), "中文模型");
    }
}
