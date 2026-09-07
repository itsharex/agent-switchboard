//! Tool definitions and tool-choice parsing for each protocol.

use super::*;

pub(super) fn parse_responses_tools(value: Option<&Value>) -> Result<Vec<Tool>, TransformError> {
    let Some(value) = value else {
        return Ok(vec![]);
    };
    let mut tools = Vec::new();
    for value in array(value, "tools")? {
        let item = object(value, "Responses tool")?;
        match string(item.get("type"), "tool.type")?.as_str() {
            "function" => {
                allowed(
                    item,
                    &["type", "name", "description", "parameters", "strict"],
                    "Responses function tool",
                )?;
                tools.push(Tool {
                    name: string(item.get("name"), "tool.name")?,
                    namespace: None,
                    description: optional_string(item, "description", "Responses function tool")?,
                    input_schema: item
                        .get("parameters")
                        .cloned()
                        .unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
                    strict: optional_bool(item, "strict")?,
                });
            }
            "namespace" => {
                allowed(
                    item,
                    &["type", "name", "description", "tools"],
                    "Responses namespace tool",
                )?;
                let namespace = string(item.get("name"), "namespace.name")?;
                let namespace_description =
                    optional_string(item, "description", "Responses namespace tool")?;
                for nested in array(
                    item.get("tools").ok_or_else(|| {
                        TransformError("Responses namespace 工具缺少 tools".to_string())
                    })?,
                    "Responses namespace tools",
                )? {
                    let nested = object(nested, "Responses namespace function")?;
                    allowed(
                        nested,
                        &["type", "name", "description", "parameters", "strict"],
                        "Responses namespace function",
                    )?;
                    if string(nested.get("type"), "namespace tool.type")? != "function" {
                        return error("Responses namespace 仅支持 type=function 的子工具");
                    }
                    tools.push(Tool {
                        name: string(nested.get("name"), "namespace tool.name")?,
                        namespace: Some(namespace.clone()),
                        description: merge_namespace_description(
                            namespace_description.as_deref(),
                            optional_string(nested, "description", "Responses namespace function")?,
                        ),
                        input_schema: nested
                            .get("parameters")
                            .cloned()
                            .unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
                        strict: optional_bool(nested, "strict")?,
                    });
                }
            }
            "web_search" => {
                return error(
                    "Responses web_search 是服务端工具，无法由 Chat Completions 或 Anthropic Messages 无损承载",
                );
            }
            other => return error(format!("Responses 工具类型 {other} 不支持跨协议转换")),
        }
    }
    Ok(tools)
}

pub(super) fn merge_namespace_description(
    namespace_description: Option<&str>,
    function_description: Option<String>,
) -> Option<String> {
    match (namespace_description, function_description) {
        (Some(namespace), Some(function)) if !namespace.is_empty() && !function.is_empty() => {
            Some(format!("{namespace}\n\n{function}"))
        }
        (Some(namespace), _) if !namespace.is_empty() => Some(namespace.to_string()),
        (_, function) => function,
    }
}

pub(super) fn parse_chat_tools(value: Option<&Value>) -> Result<Vec<Tool>, TransformError> {
    let Some(value) = value else {
        return Ok(vec![]);
    };
    let mut tools = Vec::new();
    for value in array(value, "tools")? {
        let item = object(value, "Chat tool")?;
        allowed(item, &["type", "function"], "Chat tool")?;
        if string(item.get("type"), "tool.type")? != "function" {
            return error("仅支持 type=function 的 Chat 工具");
        }
        let function = object(
            item.get("function")
                .ok_or_else(|| TransformError("Chat tool 缺少 function".to_string()))?,
            "tool.function",
        )?;
        allowed(
            function,
            &["name", "description", "parameters", "strict"],
            "tool.function",
        )?;
        tools.push(Tool {
            name: string(function.get("name"), "tool.function.name")?,
            namespace: None,
            description: optional_string(function, "description", "tool.function")?,
            input_schema: function
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
            strict: optional_bool(function, "strict")?,
        });
    }
    Ok(tools)
}

pub(super) fn parse_anthropic_tools(value: Option<&Value>) -> Result<Vec<Tool>, TransformError> {
    let Some(value) = value else {
        return Ok(vec![]);
    };
    let mut tools = Vec::new();
    for value in array(value, "tools")? {
        let item = object(value, "Anthropic tool")?;
        allowed(
            item,
            &["name", "description", "input_schema"],
            "Anthropic tool",
        )?;
        tools.push(Tool {
            name: string(item.get("name"), "tool.name")?,
            namespace: None,
            description: optional_string(item, "description", "Anthropic tool")?,
            input_schema: item
                .get("input_schema")
                .cloned()
                .ok_or_else(|| TransformError("Anthropic tool 缺少 input_schema".to_string()))?,
            strict: false,
        });
    }
    Ok(tools)
}

pub(super) fn parse_responses_tool_choice(
    value: Option<&Value>,
) -> Result<ToolChoice, TransformError> {
    match value {
        None => Ok(ToolChoice::Auto),
        Some(Value::String(value)) => parse_tool_choice_word(value),
        Some(Value::Object(map)) => {
            allowed(map, &["type", "name", "namespace"], "Responses tool_choice")?;
            if string(map.get("type"), "tool_choice.type")? != "function" {
                return error("Responses tool_choice 仅支持 function");
            }
            Ok(ToolChoice::Named {
                name: string(map.get("name"), "tool_choice.name")?,
                namespace: optional_string(map, "namespace", "Responses tool_choice")?,
            })
        }
        _ => error("Responses tool_choice 格式不支持"),
    }
}

pub(super) fn parse_chat_tool_choice(value: Option<&Value>) -> Result<ToolChoice, TransformError> {
    match value {
        None => Ok(ToolChoice::Auto),
        Some(Value::String(value)) => parse_tool_choice_word(value),
        Some(Value::Object(map)) => {
            allowed(map, &["type", "function"], "Chat tool_choice")?;
            if string(map.get("type"), "tool_choice.type")? != "function" {
                return error("Chat tool_choice 仅支持 function");
            }
            let function = object(
                map.get("function")
                    .ok_or_else(|| TransformError("Chat tool_choice 缺少 function".to_string()))?,
                "tool_choice.function",
            )?;
            allowed(function, &["name"], "tool_choice.function")?;
            Ok(ToolChoice::Named {
                name: string(function.get("name"), "tool_choice.function.name")?,
                namespace: None,
            })
        }
        _ => error("Chat tool_choice 格式不支持"),
    }
}

pub(super) fn parse_anthropic_tool_choice(
    value: Option<&Value>,
) -> Result<ToolChoice, TransformError> {
    let Some(value) = value else {
        return Ok(ToolChoice::Auto);
    };
    let map = object(value, "Anthropic tool_choice")?;
    allowed(
        map,
        &["type", "name", "disable_parallel_tool_use"],
        "Anthropic tool_choice",
    )?;
    match string(map.get("type"), "tool_choice.type")?.as_str() {
        "auto" => Ok(ToolChoice::Auto),
        "any" => Ok(ToolChoice::Required),
        "tool" => Ok(ToolChoice::Named {
            name: string(map.get("name"), "tool_choice.name")?,
            namespace: None,
        }),
        other => error(format!("Anthropic tool_choice.type {other} 不支持")),
    }
}

pub(super) fn parse_anthropic_parallel_tool_calls(
    value: Option<&Value>,
) -> Result<Option<bool>, TransformError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let map = object(value, "Anthropic tool_choice")?;
    match map.get("disable_parallel_tool_use") {
        None => Ok(None),
        Some(value) => value
            .as_bool()
            .map(|disabled| Some(!disabled))
            .ok_or_else(|| TransformError("disable_parallel_tool_use 必须是布尔值".to_string())),
    }
}

pub(super) fn parse_tool_choice_word(value: &str) -> Result<ToolChoice, TransformError> {
    match value {
        "auto" => Ok(ToolChoice::Auto),
        "required" | "any" => Ok(ToolChoice::Required),
        "none" => Ok(ToolChoice::None),
        other => error(format!("tool_choice {other} 不支持")),
    }
}
