//! The explicit minimal Responses contract. Tool and conversation semantics
//! stay intact; provider-specific optional extensions never reach upstream.

use super::{ConvertedRequest, TransformError};
use asb_core::contracts::{ResponsesOptions, ResponsesRequestMode};
use serde_json::{Map, Value};

const KEPT_FIELDS: &[&str] = &[
    "model",
    "input",
    "instructions",
    "stream",
    "max_output_tokens",
    "tools",
    "tool_choice",
    "parallel_tool_calls",
    "temperature",
    "top_p",
];
const OMITTED_FIELDS: &[&str] = &[
    "reasoning",
    "service_tier",
    "store",
    "include",
    "metadata",
    "client_metadata",
    "prompt_cache_key",
    "prompt_cache_retention",
    "safety_identifier",
];

pub(crate) fn apply(
    request: ConvertedRequest,
    options: Option<ResponsesOptions>,
) -> Result<ConvertedRequest, TransformError> {
    if options.is_none_or(|options| options.request_mode != ResponsesRequestMode::Minimal) {
        return Ok(request);
    }
    let mut value: Value = serde_json::from_slice(&request.body)
        .map_err(|_| TransformError("Responses 请求体不是有效 JSON".to_string()))?;
    let map = value
        .as_object_mut()
        .ok_or_else(|| TransformError("Responses 请求体必须是对象".to_string()))?;
    filter_fields(map)?;
    let body = serde_json::to_vec(&value)
        .map_err(|_| TransformError("无法编码最小 Responses 请求".to_string()))?;
    Ok(ConvertedRequest {
        body,
        stream: request.stream,
    })
}

pub(crate) fn filter_fields(map: &mut Map<String, Value>) -> Result<(), TransformError> {
    if map
        .get("store")
        .is_some_and(|value| !value.is_null() && value != &Value::Bool(false))
    {
        return Err(TransformError(
            "最小 Responses 模式不支持 store=true；需要显式存储语义时请选择标准模式".to_string(),
        ));
    }
    if map
        .get("model")
        .and_then(Value::as_str)
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err(TransformError(
            "最小 Responses 请求必须指定非空 model".to_string(),
        ));
    }
    if map
        .get("input")
        .is_none_or(|value| !value.is_string() && !value.is_array())
    {
        return Err(TransformError(
            "最小 Responses 请求必须指定 input 字符串或数组".to_string(),
        ));
    }
    if map
        .get("previous_response_id")
        .is_some_and(|value| !empty(value))
    {
        return Err(TransformError(
            "最小 Responses 请求不支持 previous_response_id；请提交完整 input 上下文".to_string(),
        ));
    }
    for (name, value) in map.iter() {
        if !KEPT_FIELDS.contains(&name.as_str())
            && !OMITTED_FIELDS.contains(&name.as_str())
            && !empty(value)
        {
            return Err(TransformError(format!(
                "最小 Responses 请求不支持字段 {name}"
            )));
        }
    }
    for name in ["stream", "parallel_tool_calls"] {
        if map.get(name).is_some_and(|value| !value.is_boolean()) {
            return Err(TransformError(format!("{name} 必须是布尔值")));
        }
    }
    if map
        .get("max_output_tokens")
        .is_some_and(|value| value.as_u64().is_none_or(|value| value == 0))
    {
        return Err(TransformError("max_output_tokens 必须是正整数".to_string()));
    }
    map.retain(|name, _| KEPT_FIELDS.contains(&name.as_str()));
    filter_input(map)
}

fn filter_input(map: &mut Map<String, Value>) -> Result<(), TransformError> {
    if let Some(Value::Array(input)) = map.get_mut("input") {
        for item in input {
            if let Some(item) = item.as_object_mut() {
                if item
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| {
                        !matches!(kind, "message" | "function_call" | "function_call_output")
                    })
                {
                    return Err(TransformError("最小 Responses 模式的 input 仅支持消息和工具调用记录；reasoning 或远程引用须使用标准模式，或改为完整可见上下文".to_string()));
                }
                item.remove("internal_chat_message_metadata_passthrough");
            } else {
                return Err(TransformError(
                    "Responses input 数组项必须是对象".to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(value) => value.is_empty(),
        Value::Array(value) => value.is_empty(),
        Value::Object(value) => value.is_empty(),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
