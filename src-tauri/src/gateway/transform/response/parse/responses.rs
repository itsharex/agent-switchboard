use super::*;

pub(super) fn parse_parts(
    value: &Value,
    transport: Option<&ReasoningTransport>,
    terminal: lifecycle::ResponsesTerminal,
) -> Result<Vec<ResponsePart>, TransformError> {
    let mut content = Vec::new();
    let mut identities = std::collections::BTreeSet::new();
    for value in array(value, "output")? {
        let item = object(value, "Responses output item")?;
        terminal.validate_item(item)?;
        for field in ["id", "call_id"] {
            if let Some(id) = item.get(field).and_then(Value::as_str) {
                if !identities.insert((field, id.to_string())) {
                    return error(format!("Responses output contains duplicate {field}: {id}"));
                }
            }
        }
        match string(item.get("type"), "output.type")?.as_str() {
            "message" => content.extend(parse_message(item)?),
            "function_call" => content.push(parse_function(item)?),
            "custom_tool_call" => content.push(parse_custom_tool_call(item)?),
            "tool_search_call" => content.push(parse_search(item)?),
            "reasoning" => {
                let transport = transport
                    .ok_or_else(|| TransformError("当前转换缺少本机推理续接通道".into()))?;
                content.push(ResponsePart::Reasoning(
                    transport.from_responses_item(Value::Object(item.clone()))?,
                ));
            }
            other => return error(format!("Responses output.type {other} 不支持转换")),
        }
    }
    Ok(content)
}

fn parse_message(item: &Map<String, Value>) -> Result<Vec<ResponsePart>, TransformError> {
    allowed(
        item,
        &["type", "id", "status", "role", "content"],
        "Responses message",
    )?;
    if string(item.get("role"), "message.role")? != "assistant" {
        return error("Responses output message 不是 assistant");
    }
    let mut text = String::new();
    for part in array(
        item.get("content")
            .ok_or_else(|| TransformError("Responses message 缺少 content".into()))?,
        "message.content",
    )? {
        let part = object(part, "Responses output content")?;
        let kind = string(part.get("type"), "content.type")?;
        let field = match kind.as_str() {
            "output_text" => "text",
            "refusal" => "refusal",
            other => return error(format!("Responses output content.type {other} 不支持转换")),
        };
        allowed(
            part,
            &["type", "text", "refusal", "annotations"],
            "Responses output text",
        )?;
        text.push_str(&string(part.get(field), field)?);
    }
    Ok(vec![ResponsePart::Text(text)])
}

fn parse_function(item: &Map<String, Value>) -> Result<ResponsePart, TransformError> {
    allowed(
        item,
        &[
            "type",
            "id",
            "call_id",
            "name",
            "namespace",
            "arguments",
            "status",
        ],
        "Responses function_call",
    )?;
    let arguments = string(item.get("arguments"), "function_call.arguments")?;
    let input = lifecycle::function_arguments(&arguments)?;
    Ok(ResponsePart::ToolCall {
        id: string(item.get("call_id"), "function_call.call_id")?,
        name: string(item.get("name"), "function_call.name")?,
        namespace: optional_string(item, "namespace", "Responses function_call")?,
        kind: ToolKind::Function,
        input,
    })
}

fn parse_search(item: &Map<String, Value>) -> Result<ResponsePart, TransformError> {
    allowed(
        item,
        &["type", "id", "call_id", "status", "execution", "arguments"],
        "Responses tool_search_call",
    )?;
    if item.get("execution").and_then(Value::as_str) != Some("client") {
        return error("Responses tool_search_call.execution 必须是 client");
    }
    let input = item.get("arguments").cloned().unwrap_or_else(|| json!({}));
    if !input.is_object() {
        return error("Responses tool_search_call.arguments 必须是对象");
    }
    Ok(ResponsePart::ToolCall {
        id: string(item.get("call_id"), "tool_search_call.call_id")?,
        name: CODEX_TOOL_SEARCH_NAME.to_string(),
        namespace: None,
        kind: ToolKind::ToolSearch,
        input,
    })
}
