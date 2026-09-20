//! Responses output items and terminal envelopes shared by JSON and SSE.

use super::*;
use crate::gateway::transform::Usage;

pub(super) fn render_responses(response: &CanonicalResponse) -> Result<Value, TransformError> {
    let status = responses_status(response.stop);
    let output = response.content.iter().enumerate().map(|(index, part)| {
        match part {
            ResponsePart::Text(text) => Ok(responses_text_item(&response.id, index as u64, text, status)),
            ResponsePart::Reasoning(reasoning) => {
                Ok(responses_reasoning_item(&response.id, index as u64, reasoning))
            }
            ResponsePart::ToolCall { id, name, namespace, kind, input } => {
                let input = if *kind == ToolKind::Custom {
                    json!({"input": render::custom_input(input)?})
                } else {
                    input.clone()
                };
                responses_tool_call_item(id, name, namespace.as_deref(), *kind, &input.to_string(), status)
            }
        }
    }).collect::<Result<Vec<_>, _>>()?;
    Ok(responses_envelope(&response.id, &response.model, response.stop, &response.usage, output))
}

pub(in crate::gateway::transform) fn responses_status(stop: StopReason) -> &'static str {
    if matches!(stop, StopReason::MaxTokens) { "incomplete" } else { "completed" }
}

pub(in crate::gateway::transform) fn responses_envelope(
    id: &str,
    model: &str,
    stop: StopReason,
    usage: &Usage,
    output: Vec<Value>,
) -> Value {
    let mut response = json!({
        "id": id, "object": "response", "status": responses_status(stop),
        "model": model, "output": output,
        "usage": super::super::usage::responses_json(usage), "error": null,
    });
    if matches!(stop, StopReason::MaxTokens) {
        response["incomplete_details"] = json!({"reason":"max_output_tokens"});
    }
    response
}

pub(in crate::gateway::transform) fn responses_text_item(
    response_id: &str,
    index: u64,
    text: &str,
    status: &str,
) -> Value {
    json!({
        "type":"message", "id":format!("msg_{response_id}_{index}"),
        "status":status, "role":"assistant",
        "content":[{"type":"output_text", "text":text, "annotations":[]}],
    })
}

pub(in crate::gateway::transform) fn responses_reasoning_item(
    response_id: &str,
    output_index: u64,
    reasoning: &Reasoning,
) -> Value {
    json!({
        "type":"reasoning", "id":format!("rs_{response_id}_{output_index}"),
        "status":"completed", "summary":[], "content":[],
        "encrypted_content":reasoning.continuation,
    })
}

pub(in crate::gateway::transform) fn responses_tool_call_item(
    id: &str,
    name: &str,
    namespace: Option<&str>,
    kind: ToolKind,
    arguments: &str,
    status: &str,
) -> Result<Value, TransformError> {
    let (item_type, prefix) = match kind {
        ToolKind::Function => ("function_call", Some("fc")),
        ToolKind::Custom => ("custom_tool_call", Some("ctc")),
        ToolKind::ToolSearch => ("tool_search_call", None),
    };
    let mut item = json!({"type":item_type,"call_id":id,"status":status});
    if let Some(prefix) = prefix {
        item["id"] = json!(format!("{prefix}_{id}"));
        item["name"] = json!(name);
    }
    if let Some(namespace) = namespace {
        item["namespace"] = json!(namespace);
    }
    match kind {
        ToolKind::Function => {
            if status == "completed" {
                lifecycle::function_arguments(arguments)?;
            }
            item["arguments"] = json!(arguments);
        }
        ToolKind::Custom => {
            item["input"] = if arguments.is_empty() && status == "in_progress" {
                json!("")
            } else {
                let input: Value = serde_json::from_str(arguments).map_err(|_| {
                    TransformError("custom 工具参数不是完整 JSON，无法无损恢复工具输入".into())
                })?;
                let input = input.as_object().filter(|value| value.len() == 1)
                    .and_then(|value| value.get("input")).and_then(Value::as_str)
                    .ok_or_else(|| TransformError("custom 工具参数必须是唯一 input 字符串".into()))?;
                json!(input)
            };
        }
        ToolKind::ToolSearch => {
            let arguments = if arguments.is_empty() && status == "in_progress" {
                json!({})
            } else {
                lifecycle::function_arguments(arguments)?
            };
            item["execution"] = json!("client");
            item["arguments"] = arguments;
        }
    }
    Ok(item)
}
