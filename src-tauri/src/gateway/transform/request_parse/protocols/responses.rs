//! Responses request parsing and full visible tool-history validation.

use super::*;

mod history;
mod output;
mod validation;
use history::{ResponseToolCall, ResponseToolHistory};
use output::parse_tool_output;
use validation::{
    optional_nonempty_string, validate_completed_client_item, validate_output_identity,
    validate_replay_metadata,
};

struct ResponsesInput {
    system: Vec<Part>,
    messages: Vec<Message>,
    tools: Vec<Tool>,
    history: ResponseToolHistory,
}

pub(crate) fn parse_responses(
    value: &Value,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<CanonicalRequest, TransformError> {
    let map = object(value, "Responses 请求")?;
    allowed(map, RESPONSES_REQUEST_FIELDS, "Responses 请求")?;
    validate_responses_transport_metadata(map)?;
    let system = match map.get("instructions") {
        None => vec![],
        Some(Value::String(text)) => vec![Part::Text(text.clone())],
        Some(_) => return error("instructions 必须是字符串"),
    };
    let mut input = ResponsesInput {
        system,
        messages: Vec::new(),
        tools: parse_responses_tools(map.get("tools"))?,
        history: ResponseToolHistory::default(),
    };
    parse_responses_input(
        map.get("input")
            .ok_or_else(|| TransformError("Responses 请求缺少 input".to_string()))?,
        &mut input,
        reasoning_transport,
    )?;
    if input.messages.is_empty() && input.system.is_empty() {
        return error("Responses 请求不包含可转换的输入内容");
    }
    Ok(CanonicalRequest {
        model: string(map.get("model"), "model")?,
        system: input.system,
        messages: input.messages,
        tools: input.tools,
        tool_choice: parse_responses_tool_choice(map.get("tool_choice"))?,
        parallel_tool_calls: optional_bool_option(map, "parallel_tool_calls")?,
        stream: optional_bool(map, "stream")?,
        max_tokens: optional_u64(map, "max_output_tokens")?,
        temperature: optional_number(map, "temperature")?,
        top_p: optional_number(map, "top_p")?,
        stop: None,
        user_id: parse_user_metadata(map.get("metadata"), "Responses metadata")?,
        reasoning_effort: None,
    })
}

const RESPONSES_REQUEST_FIELDS: &[&str] = &[
    "model",
    "input",
    "instructions",
    "tools",
    "tool_choice",
    "parallel_tool_calls",
    "stream",
    "max_output_tokens",
    "temperature",
    "top_p",
    "client_metadata",
    "include",
    "prompt_cache_key",
    "reasoning",
    "store",
    "metadata",
];

fn parse_responses_input(
    value: &Value,
    input: &mut ResponsesInput,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<(), TransformError> {
    match value {
        Value::String(text) => input.messages.push(Message {
            role: Role::User,
            parts: vec![Part::Text(text.clone())],
        }),
        _ => {
            for item in array(value, "input")? {
                parse_responses_input_item(object(item, "input 项")?, input, reasoning_transport)?;
            }
        }
    }
    Ok(())
}

fn parse_responses_input_item(
    item: &Map<String, Value>,
    input: &mut ResponsesInput,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<(), TransformError> {
    let kind = string(item.get("type"), "input.type")?;
    match kind.as_str() {
        "message" => parse_responses_message(item, input),
        "function_call" => parse_function_call(item, input),
        "function_call_output" => parse_function_call_output(item, input),
        "custom_tool_call" => parse_custom_tool_call(item, input),
        "custom_tool_call_output" => parse_custom_tool_call_output(item, input),
        "tool_search_call" => parse_tool_search_call(item, input),
        "tool_search_output" => parse_tool_search_output(item, input),
        "reasoning" => parse_responses_reasoning(item, input, reasoning_transport),
        _ => error(format!("Responses input.type {kind} 不支持跨协议转换")),
    }
}

fn parse_responses_message(
    item: &Map<String, Value>,
    input: &mut ResponsesInput,
) -> Result<(), TransformError> {
    allowed(
        item,
        &[
            "type",
            "id",
            "status",
            "role",
            "content",
            "internal_chat_message_metadata_passthrough",
        ],
        "Responses message",
    )?;
    validate_replay_metadata(item, "Responses message")?;
    if let Some(metadata) = item.get("internal_chat_message_metadata_passthrough") {
        if !metadata.is_object() {
            return error("internal_chat_message_metadata_passthrough 必须是对象");
        }
    }
    let role = parse_role(&string(item.get("role"), "input.role")?)?;
    let parts = parse_response_content(
        item.get("content")
            .ok_or_else(|| TransformError("Responses message 缺少 content".to_string()))?,
    )?;
    if matches!(role, Role::System | Role::Developer) {
        input.system.extend(parts);
    } else if role == Role::Assistant {
        append_assistant_parts(&mut input.messages, parts);
    } else {
        input.messages.push(Message { role, parts });
    }
    Ok(())
}

fn parse_function_call(
    item: &Map<String, Value>,
    input: &mut ResponsesInput,
) -> Result<(), TransformError> {
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
        "function_call",
    )?;
    validate_replay_metadata(item, "function_call")?;
    let call_id = string(item.get("call_id"), "function_call.call_id")?;
    let name = string(item.get("name"), "function_call.name")?;
    let namespace = optional_nonempty_string(item, "namespace", "function_call")?;
    let arguments = parse_function_arguments(item)?;
    input.history.record(
        call_id.clone(),
        ResponseToolCall {
            kind: ToolKind::Function,
            name: name.clone(),
            namespace: namespace.clone(),
        },
    )?;
    append_assistant_parts(
        &mut input.messages,
        vec![Part::ToolCall {
            id: call_id,
            name,
            namespace,
            kind: ToolKind::Function,
            input: arguments,
        }],
    );
    Ok(())
}

fn parse_function_call_output(
    item: &Map<String, Value>,
    input: &mut ResponsesInput,
) -> Result<(), TransformError> {
    allowed(
        item,
        &[
            "type",
            "id",
            "call_id",
            "name",
            "namespace",
            "output",
            "status",
        ],
        "function_call_output",
    )?;
    validate_replay_metadata(item, "function_call_output")?;
    let call_id = string(item.get("call_id"), "function_call_output.call_id")?;
    let call = input
        .history
        .take_output(&call_id, ToolKind::Function, "function_call_output")?;
    validate_output_identity(item, &call, "function_call_output")?;
    let content = parse_tool_output(item, "function_call_output")?;
    input.messages.push(Message {
        role: Role::User,
        parts: vec![Part::ToolResult {
            id: call_id,
            kind: ToolKind::Function,
            content,
            is_error: false,
        }],
    });
    Ok(())
}

fn parse_custom_tool_call(
    item: &Map<String, Value>,
    input: &mut ResponsesInput,
) -> Result<(), TransformError> {
    allowed(
        item,
        &[
            "type",
            "id",
            "call_id",
            "name",
            "namespace",
            "input",
            "status",
        ],
        "custom_tool_call",
    )?;
    validate_replay_metadata(item, "custom_tool_call")?;
    let call_id = string(item.get("call_id"), "custom_tool_call.call_id")?;
    let name = string(item.get("name"), "custom_tool_call.name")?;
    let namespace = optional_nonempty_string(item, "namespace", "custom_tool_call")?;
    let tool_input = string(item.get("input"), "custom_tool_call.input")?;
    input.history.record(
        call_id.clone(),
        ResponseToolCall {
            kind: ToolKind::Custom,
            name: name.clone(),
            namespace: namespace.clone(),
        },
    )?;
    append_assistant_parts(
        &mut input.messages,
        vec![Part::ToolCall {
            id: call_id,
            name,
            namespace,
            kind: ToolKind::Custom,
            input: Value::String(tool_input),
        }],
    );
    Ok(())
}

fn parse_custom_tool_call_output(
    item: &Map<String, Value>,
    input: &mut ResponsesInput,
) -> Result<(), TransformError> {
    allowed(
        item,
        &["type", "id", "call_id", "output", "status"],
        "custom_tool_call_output",
    )?;
    validate_replay_metadata(item, "custom_tool_call_output")?;
    let call_id = string(item.get("call_id"), "custom_tool_call_output.call_id")?;
    input
        .history
        .take_output(&call_id, ToolKind::Custom, "custom_tool_call_output")?;
    parse_tool_output(item, "custom_tool_call_output")?;
    let serialized = serde_json::to_string(item)
        .map_err(|_| TransformError("无法编码 custom_tool_call_output".to_string()))?;
    input.messages.push(Message {
        role: Role::User,
        parts: vec![Part::ToolResult {
            id: call_id,
            kind: ToolKind::Custom,
            content: vec![Part::Text(serialized)],
            is_error: false,
        }],
    });
    Ok(())
}

fn parse_tool_search_call(
    item: &Map<String, Value>,
    input: &mut ResponsesInput,
) -> Result<(), TransformError> {
    allowed(
        item,
        &["type", "id", "call_id", "status", "execution", "arguments"],
        "tool_search_call",
    )?;
    validate_completed_client_item(item, "tool_search_call")?;
    let call_id = string(item.get("call_id"), "tool_search_call.call_id")?;
    let arguments = item
        .get("arguments")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or_else(|| TransformError("tool_search_call.arguments 必须是对象".to_string()))?;
    input.history.record(
        call_id.clone(),
        ResponseToolCall {
            kind: ToolKind::ToolSearch,
            name: CODEX_TOOL_SEARCH_NAME.to_string(),
            namespace: None,
        },
    )?;
    append_assistant_parts(
        &mut input.messages,
        vec![Part::ToolCall {
            id: call_id,
            name: CODEX_TOOL_SEARCH_NAME.to_string(),
            namespace: None,
            kind: ToolKind::ToolSearch,
            input: arguments,
        }],
    );
    Ok(())
}

fn parse_tool_search_output(
    item: &Map<String, Value>,
    input: &mut ResponsesInput,
) -> Result<(), TransformError> {
    allowed(
        item,
        &[
            "type",
            "id",
            "call_id",
            "status",
            "execution",
            "tools",
            "output",
        ],
        "tool_search_output",
    )?;
    validate_replay_metadata(item, "tool_search_output")?;
    if let Some(execution) = item.get("execution") {
        if execution.as_str() != Some("client") {
            return error("tool_search_output.execution 必须是 client");
        }
    }
    let call_id = string(item.get("call_id"), "tool_search_output.call_id")?;
    input
        .history
        .take_output(&call_id, ToolKind::ToolSearch, "tool_search_output")?;
    let tools = item
        .get("tools")
        .ok_or_else(|| TransformError("tool_search_output 缺少 tools".to_string()))?;
    input.tools.extend(parse_responses_tools(Some(tools))?);
    // The public Responses schema does not currently expose `output` on a
    // tool-search result, while Codex includes it as a client-result payload.
    // Validate and retain it when present so the result remains visible to a
    // Chat/Anthropic upstream instead of being rejected as an unknown field.
    if item.get("output").is_some() {
        parse_tool_output(item, "tool_search_output")?;
    }
    let serialized = serde_json::to_string(item)
        .map_err(|_| TransformError("无法编码 tool_search_output".to_string()))?;
    input.messages.push(Message {
        role: Role::User,
        parts: vec![Part::ToolResult {
            id: call_id,
            kind: ToolKind::ToolSearch,
            content: vec![Part::Text(serialized)],
            is_error: false,
        }],
    });
    Ok(())
}

fn parse_responses_reasoning(
    item: &Map<String, Value>,
    input: &mut ResponsesInput,
    reasoning_transport: Option<&ReasoningTransport>,
) -> Result<(), TransformError> {
    allowed(
        item,
        &[
            "type",
            "id",
            "summary",
            "content",
            "encrypted_content",
            "status",
        ],
        "Responses reasoning",
    )?;
    validate_replay_metadata(item, "Responses reasoning")?;
    if let Some(summary) = item.get("summary") {
        if !summary.is_array() {
            return error("Responses reasoning.summary 必须是数组");
        }
    }
    if let Some(content) = item.get("content") {
        if content.as_array().is_none_or(|parts| !parts.is_empty()) {
            return error("Responses reasoning.content 必须是空数组");
        }
    }
    let continuation = string(item.get("encrypted_content"), "reasoning.encrypted_content")?;
    let transport = reasoning_transport
        .ok_or_else(|| TransformError("当前转换缺少本机推理续接通道".to_string()))?;
    append_assistant_parts(
        &mut input.messages,
        vec![Part::Reasoning(transport.from_continuation(continuation)?)],
    );
    Ok(())
}

fn parse_function_arguments(item: &Map<String, Value>) -> Result<Value, TransformError> {
    let arguments = string(item.get("arguments"), "function_call.arguments")?;
    let input: Value = serde_json::from_str(&arguments)
        .map_err(|_| TransformError("function_call.arguments 必须是 JSON 对象".to_string()))?;
    if !input.is_object() {
        return error("function_call.arguments 必须是 JSON 对象");
    }
    Ok(input)
}
