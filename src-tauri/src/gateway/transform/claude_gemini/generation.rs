use super::*;

/// Keeps explicit Claude budgets distinct from effort; the generic canonical
/// effort is intentionally not used to guess a budget for Google models.
pub(in crate::gateway::transform) fn apply(
    source: &Value,
    body: &mut Value,
) -> Result<(), TransformError> {
    if let Some(top_k) = source.get("top_k") {
        if top_k.as_u64().is_none_or(|v| v == 0) {
            return error("Gemini top_k 必须是正整数");
        }
        body["generationConfig"]["topK"] = top_k.clone();
    }
    let thinking = source.get("thinking");
    let effort = source
        .pointer("/output_config/effort")
        .and_then(Value::as_str);
    if thinking.is_none() && effort.is_none() {
        return Ok(());
    }
    let model = source
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim_start_matches("models/");
    let disabled = thinking.and_then(|v| v.get("type")).and_then(Value::as_str) == Some("disabled");
    let budget = thinking
        .and_then(|v| v.get("budget_tokens"))
        .and_then(Value::as_u64);
    let config = if model.starts_with("gemini-3") {
        if disabled && !model.contains("flash") {
            return error("此 Gemini 模型不能关闭思考，请选择思考强度");
        }
        let level = if disabled {
            "minimal"
        } else {
            match effort {
                Some("low") => "low",
                Some("medium") if model.contains("flash") => "medium",
                Some(_) => "high",
                None if budget.is_some_and(|v| v < 4_000) => "low",
                None => "high",
            }
        };
        json!({"thinkingLevel":level,"includeThoughts":true})
    } else {
        let budget = if disabled {
            0_i64
        } else if let Some(budget) = budget {
            i64::try_from(budget).map_err(|_| TransformError("Gemini 思考预算超出范围".into()))?
        } else {
            match effort {
                Some("low") => 1024,
                Some("medium") => 8192,
                Some("high") => 16384,
                _ => -1,
            }
        };
        json!({"thinkingBudget":budget,"includeThoughts":true})
    };
    body["generationConfig"]["thinkingConfig"] = config;
    Ok(())
}
