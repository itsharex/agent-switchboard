//! Per-model reasoning mapping is shared by HTTP, compact and WebSocket admission.
use asb_core::contracts::{
    CodexCatalogEntry, CodexChatEffortMode, CodexChatReasoning, CodexReasoningLevel,
    CodexRouteSnapshot, CODEX_REASONING_LADDER,
};
use serde_json::{Map, Value};

pub(super) fn apply(
    snapshot: &CodexRouteSnapshot,
    model: &CodexCatalogEntry,
    request: &mut Map<String, Value>,
) -> Result<(), String> {
    if !matches!(
        snapshot.capabilities.chat_reasoning,
        CodexChatReasoning::Configured {
            effort_mode: CodexChatEffortMode::Catalog,
            ..
        }
    ) {
        return Ok(());
    }
    let Some(effort) = request
        .get_mut("reasoning")
        .and_then(Value::as_object_mut)
        .and_then(|reasoning| reasoning.get_mut("effort"))
    else {
        return Ok(());
    };
    let requested = effort
        .as_str()
        .ok_or("Codex reasoning.effort 必须是字符串")?;
    *effort = Value::String(map_level(requested, &model.supported_reasoning_levels)?);
    Ok(())
}

fn map_level(requested: &str, levels: &[CodexReasoningLevel]) -> Result<String, String> {
    let requested = requested.to_ascii_lowercase();
    if matches!(requested.as_str(), "none" | "off" | "disabled") {
        return levels
            .contains(&CodexReasoningLevel::None)
            .then(|| "none".into())
            .ok_or_else(|| "所选 Codex 模型不能关闭推理".into());
    }
    let level: CodexReasoningLevel =
        serde_json::from_value(Value::String(requested)).map_err(|_| "Codex 推理档位不受支持")?;
    let rank = |value: &CodexReasoningLevel| {
        CODEX_REASONING_LADDER
            .iter()
            .position(|v| v == value)
            .unwrap_or(0)
    };
    let supported = levels
        .iter()
        .filter(|v| **v != CodexReasoningLevel::None)
        .collect::<Vec<_>>();
    let selected = supported
        .iter()
        .filter(|v| rank(v) >= rank(&level))
        .min_by_key(|v| rank(v))
        .or_else(|| supported.iter().max_by_key(|v| rank(v)))
        .ok_or("所选 Codex 模型未声明可用推理档位")?;
    Ok(serde_json::to_value(selected)
        .map_err(|_| "无法序列化推理档位")?
        .as_str()
        .ok_or("无法序列化推理档位")?
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_effort_uses_the_requested_models_ladder() {
        use CodexReasoningLevel::*;
        assert_eq!(map_level("medium", &[High, Max]).unwrap(), "high");
        assert_eq!(map_level("ultra", &[High, Max]).unwrap(), "max");
        assert_eq!(map_level("medium", &[Low, Medium, High]).unwrap(), "medium");
        assert_eq!(map_level("minimal", &[Low, High]).unwrap(), "low");
        assert_eq!(map_level("none", &[None, High]).unwrap(), "none");
        assert!(map_level("none", &[High, Max]).is_err());
        assert!(map_level("unexpected", &[High, Max]).is_err());
    }
}
