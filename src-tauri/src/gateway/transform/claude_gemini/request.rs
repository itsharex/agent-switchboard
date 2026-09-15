use super::calls::Calls;
use super::*;

pub(in crate::gateway::transform) fn render(
    request: &CanonicalRequest,
) -> Result<Value, TransformError> {
    if request.parallel_tool_calls == Some(false) && !request.tools.is_empty() {
        return error("Gemini Native 不能保证 disable_parallel_tool_use；请使用自动工具策略");
    }
    let mut body = json!({"contents":[]});
    let mut system = Vec::new();
    for part in &request.system {
        system.extend(super::media::part(part)?);
    }
    let mut calls = Calls::default();
    let mut contents: Vec<Value> = Vec::new();
    for message in &request.messages {
        if matches!(message.role, Role::System | Role::Developer) {
            for part in &message.parts {
                system.extend(super::media::part(part)?);
            }
            continue;
        }
        let parts = message_parts(&message.parts, message.role, &mut calls, &request.model)?;
        if parts.is_empty() {
            continue;
        }
        let role = if message.role == Role::Assistant {
            "model"
        } else {
            "user"
        };
        if contents.last().is_some_and(|last| last["role"] == role) {
            contents.last_mut().expect("last exists")["parts"]
                .as_array_mut()
                .expect("array")
                .extend(parts);
        } else {
            contents.push(json!({"role":role,"parts":parts}));
        }
    }
    if contents.is_empty() {
        return error("Gemini 请求缺少对话内容");
    }
    body["contents"] = json!(contents);
    if !system.is_empty() {
        body["systemInstruction"] = json!({"parts":system});
    }
    if !request.tools.is_empty() {
        let tools = request
            .tools
            .iter()
            .map(|tool| {
                if tool.strict || tool.kind != ToolKind::Function || tool.namespace.is_some() {
                    return error("Gemini Native 只支持 Claude JSON 函数工具");
                }
                validate_name(&tool.name)?;
                let mut value = json!({"name":tool.name,"parametersJsonSchema":tool.input_schema});
                if let Some(description) = &tool.description {
                    value["description"] = json!(description);
                }
                Ok(value)
            })
            .collect::<Result<Vec<_>, _>>()?;
        body["tools"] = json!([{"functionDeclarations":tools}]);
        let config = match &request.tool_choice {
            ToolChoice::Auto => json!({"mode":"AUTO"}),
            ToolChoice::Required => json!({"mode":"ANY"}),
            ToolChoice::None => json!({"mode":"NONE"}),
            ToolChoice::Named { name, .. } => json!({"mode":"ANY","allowedFunctionNames":[name]}),
        };
        body["toolConfig"] = json!({"functionCallingConfig":config});
    } else if matches!(
        request.tool_choice,
        ToolChoice::Required | ToolChoice::Named { .. }
    ) {
        return error("Gemini 指定工具策略要求非空工具列表");
    }
    let mut generation = json!({});
    if let Some(max) = request.max_tokens {
        generation["maxOutputTokens"] = json!(max);
    }
    if let Some(value) = &request.temperature {
        generation["temperature"] = value.clone();
    }
    if let Some(value) = &request.top_p {
        generation["topP"] = value.clone();
    }
    if let Some(value) = &request.stop {
        generation["stopSequences"] = json!(value);
    }
    body["generationConfig"] = generation;
    Ok(body)
}

fn message_parts(
    parts: &[Part],
    role: Role,
    calls: &mut Calls,
    model: &str,
) -> Result<Vec<Value>, TransformError> {
    let turns = parts
        .iter()
        .filter_map(|part| match part {
            Part::Reasoning(r) => Some(r),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !turns.is_empty() {
        if role != Role::Assistant || turns.len() != 1 {
            return error("Gemini 签名续接必须属于同一条 assistant 消息");
        }
        let turn = turns[0]
            .gemini_turn
            .as_ref()
            .ok_or_else(|| TransformError("不能将其它上游的推理记录回放到 Gemini".into()))?;
        let restored = turn.restore(parts)?;
        for (raw, id) in restored
            .iter()
            .filter_map(|part| part.get("functionCall"))
            .zip(&turn.call_ids)
        {
            calls.remember(
                id,
                raw["name"].as_str().unwrap_or_default(),
                raw.get("id").and_then(Value::as_str).map(str::to_string),
            )?;
        }
        return Ok(restored);
    }
    let mut result = Vec::new();
    for part in calls.ordered(parts) {
        match part {
            Part::ToolCall {
                id,
                name,
                input,
                kind,
                namespace,
            } => {
                if role != Role::Assistant || *kind != ToolKind::Function || namespace.is_some() {
                    return error("Gemini 工具调用角色或类型无效");
                }
                validate_name(name)?;
                calls.remember(id, name, None)?;
                result.push(json!({"functionCall":{"name":name,"args":input}}));
            }
            Part::ToolResult {
                id,
                content,
                is_error,
                ..
            } => {
                if role != Role::User {
                    return error("Gemini 工具结果必须属于 user 消息");
                }
                let call = calls.resolve(id)?;
                result.extend(super::media::tool_result(
                    &call.name,
                    call.upstream_id.as_deref(),
                    content,
                    *is_error,
                    model,
                )?);
            }
            other => result.extend(super::media::part(other)?),
        }
    }
    Ok(result)
}

fn validate_name(name: &str) -> Result<(), TransformError> {
    if name.len() > 64
        || name.is_empty()
        || !name.chars().enumerate().all(|(i, c)| {
            c.is_ascii_alphabetic()
                || c == '_'
                || (i > 0 && (c.is_ascii_digit() || matches!(c, '.' | ':' | '-')))
        })
    {
        return error("Gemini 函数名必须是 1–64 个字母、数字或 _ . : -，且以字母或下划线开头");
    }
    Ok(())
}
