use super::replay::{call_id, visible, GeminiTurn};
use super::*;

pub(in crate::gateway::transform) fn parse(
    value: &Value,
    transport: Option<&ReasoningTransport>,
) -> Result<CanonicalResponse, TransformError> {
    let candidate =
        candidate(value)?.ok_or_else(|| TransformError("Gemini 响应没有候选消息".into()))?;
    let parts = candidate
        .pointer("/content/parts")
        .and_then(Value::as_array)
        .ok_or_else(|| TransformError("Gemini 响应缺少 content.parts".into()))?;
    let turn = GeminiTurn {
        parts: parts.clone(),
        call_ids: parts.iter().filter_map(call_id).collect(),
    };
    turn.validate()?;
    let mut content = visible(parts, &turn.call_ids)?;
    let stop = stop(candidate, !turn.call_ids.is_empty())?
        .ok_or_else(|| TransformError("Gemini 响应缺少终止原因".into()))?;
    if content.is_empty() {
        return error("Gemini 响应只有推理或空内容，未返回可见回复");
    }
    let transport =
        transport.ok_or_else(|| TransformError("Gemini 转换缺少本机续接通道".into()))?;
    content.insert(
        0,
        ResponsePart::Reasoning(transport.from_gemini_turn(turn)?),
    );
    Ok(CanonicalResponse {
        id: identity(value, "responseId")?
            .unwrap_or_else(|| format!("msg_{}", uuid::Uuid::new_v4().simple())),
        model: identity(value, "modelVersion")?.unwrap_or_default(),
        content,
        stop,
        usage: super::super::usage::parse(
            asb_core::UpstreamProtocol::GeminiGenerateContent,
            value.get("usageMetadata"),
        )?,
    })
}

pub(super) fn candidate(value: &Value) -> Result<Option<&Value>, TransformError> {
    if !value.is_object() {
        return error("Gemini 响应必须是对象");
    }
    if value.get("error").is_some_and(|error| !error.is_null()) {
        return error("Gemini 返回上游错误");
    }
    if value
        .pointer("/promptFeedback/blockReason")
        .is_some_and(|reason| {
            reason
                .as_str()
                .is_some_and(|v| !v.is_empty() && v != "BLOCK_REASON_UNSPECIFIED")
        })
    {
        return error("Gemini 拒绝了请求，请检查上游安全策略和输入");
    }
    let Some(candidates) = value.get("candidates") else {
        return Ok(None);
    };
    let candidates = candidates
        .as_array()
        .ok_or_else(|| TransformError("Gemini candidates 必须是数组".into()))?;
    if candidates.len() > 1 {
        return error("Claude 转换仅支持一个 Gemini 候选消息");
    }
    if let Some(candidate) = candidates.first() {
        if candidate
            .get("index")
            .is_some_and(|index| index.as_u64() != Some(0))
        {
            return error("Gemini 候选序号无效");
        }
        if candidate
            .pointer("/content/role")
            .is_some_and(|role| role.as_str() != Some("model"))
        {
            return error("Gemini 回复角色不是 model");
        }
    }
    Ok(candidates.first())
}

pub(super) fn stop(candidate: &Value, tools: bool) -> Result<Option<StopReason>, TransformError> {
    match candidate.get("finishReason").and_then(Value::as_str) {
        None | Some("FINISH_REASON_UNSPECIFIED") => Ok(None),
        Some("STOP") => Ok(Some(if tools {
            StopReason::ToolUse
        } else {
            StopReason::EndTurn
        })),
        Some("MAX_TOKENS") => Ok(Some(StopReason::MaxTokens)),
        Some(
            "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" | "IMAGE_SAFETY",
        ) => error("Gemini 安全策略终止了生成，未返回完整回复"),
        Some("MALFORMED_FUNCTION_CALL" | "UNEXPECTED_TOOL_CALL") => {
            error("Gemini 返回无效工具调用，未完成生成")
        }
        Some(_) => error("Gemini 返回不支持的终止原因，未完成生成"),
    }
}

pub(super) fn identity(value: &Value, key: &str) -> Result<Option<String>, TransformError> {
    value
        .get(key)
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_str()
                .filter(|s| {
                    !s.trim().is_empty() && s.len() <= 512 && !s.chars().any(char::is_control)
                })
                .map(str::to_string)
                .ok_or_else(|| TransformError(format!("Gemini {key} 无效")))
        })
        .transpose()
}
