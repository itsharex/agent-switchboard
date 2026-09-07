//! Common request-root fields and tool rendering helpers.

use super::*;

pub(super) fn insert_common_chat(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<(), TransformError> {
    if !request.tools.is_empty() {
        root.insert(
            "tools".to_string(),
            Value::Array(
                request
                    .tools
                    .iter()
                    .map(|tool| -> Result<Value, TransformError> {
                        let mut function = Map::new();
                        function.insert(
                            "name".to_string(),
                            Value::String(render_target_name(
                                UpstreamProtocol::ChatCompletions,
                                tool.namespace.as_deref(),
                                &tool.name,
                            )?),
                        );
                        if let Some(description) = &tool.description {
                            function.insert(
                                "description".to_string(),
                                Value::String(description.clone()),
                            );
                        }
                        function.insert("parameters".to_string(), tool.input_schema.clone());
                        if tool.strict {
                            function.insert("strict".to_string(), Value::Bool(true));
                        }
                        Ok(json!({ "type": "function", "function": function }))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        );
        root.insert(
            "tool_choice".to_string(),
            render_chat_tool_choice(&request.tool_choice)?,
        );
    }
    root.insert("stream".to_string(), Value::Bool(request.stream));
    if let Some(value) = request.parallel_tool_calls {
        root.insert("parallel_tool_calls".to_string(), Value::Bool(value));
    }
    if let Some(value) = request.max_tokens {
        root.insert("max_tokens".to_string(), Value::Number(value.into()));
    }
    if let Some(value) = &request.temperature {
        root.insert("temperature".to_string(), value.clone());
    }
    if let Some(value) = &request.top_p {
        root.insert("top_p".to_string(), value.clone());
    }
    if let Some(value) = &request.stop {
        root.insert(
            "stop".to_string(),
            Value::Array(value.iter().cloned().map(Value::String).collect()),
        );
    }
    Ok(())
}

pub(super) fn insert_common_anthropic(root: &mut Map<String, Value>, request: &CanonicalRequest) {
    root.insert("stream".to_string(), Value::Bool(request.stream));
    if let Some(value) = &request.temperature {
        root.insert("temperature".to_string(), value.clone());
    }
    if let Some(value) = &request.top_p {
        root.insert("top_p".to_string(), value.clone());
    }
    if let Some(value) = &request.stop {
        root.insert(
            "stop_sequences".to_string(),
            Value::Array(value.iter().cloned().map(Value::String).collect()),
        );
    }
}

pub(super) fn render_responses_tools(tools: &[Tool]) -> Value {
    let mut values = Vec::new();
    let mut namespaces = std::collections::BTreeMap::<String, usize>::new();
    for tool in tools {
        match &tool.namespace {
            None => values.push(render_responses_function(tool)),
            Some(namespace) => {
                if let Some(index) = namespaces.get(namespace).copied() {
                    values[index]
                        .as_object_mut()
                        .and_then(|tool| tool.get_mut("tools"))
                        .and_then(Value::as_array_mut)
                        .expect("namespace tool is constructed with a tools array")
                        .push(render_responses_function(tool));
                    continue;
                }
                let index = values.len();
                namespaces.insert(namespace.clone(), index);
                values.push(json!({
                    "type": "namespace",
                    "name": namespace,
                    "tools": [render_responses_function(tool)],
                }));
            }
        }
    }
    Value::Array(values)
}

pub(super) fn render_responses_function(tool: &Tool) -> Value {
    let mut value = Map::new();
    value.insert("type".to_string(), Value::String("function".to_string()));
    value.insert("name".to_string(), Value::String(tool.name.clone()));
    if let Some(description) = &tool.description {
        value.insert(
            "description".to_string(),
            Value::String(description.clone()),
        );
    }
    value.insert("parameters".to_string(), tool.input_schema.clone());
    if tool.strict {
        value.insert("strict".to_string(), Value::Bool(true));
    }
    Value::Object(value)
}

pub(super) fn render_responses_tool_choice(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => Value::String("auto".to_string()),
        ToolChoice::Required => Value::String("required".to_string()),
        ToolChoice::None => Value::String("none".to_string()),
        ToolChoice::Named { name, namespace } => {
            let mut value = Map::new();
            value.insert("type".to_string(), Value::String("function".to_string()));
            value.insert("name".to_string(), Value::String(name.clone()));
            if let Some(namespace) = namespace {
                value.insert("namespace".to_string(), Value::String(namespace.clone()));
            }
            Value::Object(value)
        }
    }
}

pub(super) fn render_chat_tool_choice(choice: &ToolChoice) -> Result<Value, TransformError> {
    Ok(match choice {
        ToolChoice::Auto => Value::String("auto".to_string()),
        ToolChoice::Required => Value::String("required".to_string()),
        ToolChoice::None => Value::String("none".to_string()),
        ToolChoice::Named { name, namespace } => json!({
            "type": "function",
            "function": { "name": render_target_name(UpstreamProtocol::ChatCompletions, namespace.as_deref(), name)? },
        }),
    })
}

pub(super) fn render_anthropic_tool_choice(
    choice: &ToolChoice,
    parallel_tool_calls: Option<bool>,
) -> Result<Value, TransformError> {
    let mut value = match choice {
        ToolChoice::Auto => json!({ "type": "auto" }),
        ToolChoice::Required => json!({ "type": "any" }),
        ToolChoice::Named { name, namespace } => json!({
            "type": "tool",
            "name": render_target_name(
                UpstreamProtocol::AnthropicMessages,
                namespace.as_deref(),
                name,
            )?,
        }),
        ToolChoice::None => return error("Anthropic Messages 无法无损表达 tool_choice=none"),
    };
    if parallel_tool_calls == Some(false) {
        value
            .as_object_mut()
            .expect("tool choice is always an object")
            .insert("disable_parallel_tool_use".to_string(), Value::Bool(true));
    }
    Ok(value)
}
