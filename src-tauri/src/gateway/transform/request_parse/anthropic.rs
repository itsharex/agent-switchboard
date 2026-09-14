//! Anthropic blocks share cache validation, including nested tool results.

use super::*;

pub(super) fn parse_anthropic_content(
    value: &Value,
    role: Role,
    reasoning_transport: Option<&ReasoningTransport>,
    tools: &[Tool],
) -> Result<Vec<Part>, TransformError> {
    if let Some(text) = value.as_str() {
        return Ok(vec![Part::Text(text.to_string())]);
    }
    array(value, "Anthropic content")?
        .iter()
        .map(|value| {
            parse_anthropic_part(
                object(value, "Anthropic content item")?,
                role,
                reasoning_transport,
                tools,
            )
        })
        .collect()
}

fn parse_anthropic_part(
    item: &Map<String, Value>,
    role: Role,
    transport: Option<&ReasoningTransport>,
    tools: &[Tool],
) -> Result<Part, TransformError> {
    if let Some(cache_control) = item.get("cache_control") {
        validate_anthropic_cache_control(cache_control)?;
    }
    match string(item.get("type"), "content.type")?.as_str() {
        "text" => {
            allowed(
                item,
                &["type", "text", "cache_control"],
                "Anthropic text content",
            )?;
            let text = optional_string(item, "text", "Anthropic content")?
                .ok_or_else(|| TransformError("Anthropic content.text is required".to_string()))?;
            Ok(Part::Text(text))
        }
        "image" => parse_image(item).map(Part::Image),
        "tool_use" => parse_tool_use(item),
        "tool_result" => parse_tool_result(item, transport, tools),
        "document" => super::claude_media::parse_document(item).map(Part::Document),
        "tool_reference" => {
            super::claude_media::parse_reference(item, tools).map(Part::ToolReference)
        }
        "thinking" | "redacted_thinking" => parse_thinking(item, role, transport),
        other => error(format!("Unsupported Anthropic content.type: {other}")),
    }
}

fn parse_image(item: &Map<String, Value>) -> Result<ImageSource, TransformError> {
    allowed(
        item,
        &["type", "source", "cache_control"],
        "Anthropic image content",
    )?;
    let source = object(
        item.get("source")
            .ok_or_else(|| TransformError("Anthropic image.source is required".to_string()))?,
        "image.source",
    )?;
    match string(source.get("type"), "image.source.type")?.as_str() {
        "base64" => {
            allowed(
                source,
                &["type", "media_type", "data"],
                "Anthropic image source",
            )?;
            Ok(ImageSource::Data {
                media_type: string(source.get("media_type"), "image.source.media_type")?,
                data: string(source.get("data"), "image.source.data")?,
            })
        }
        "url" => {
            allowed(source, &["type", "url"], "Anthropic image source")?;
            let url = string(source.get("url"), "image.source.url")?;
            if !reqwest::Url::parse(&url).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
            {
                return error("Anthropic image.source.url must be an HTTP(S) URL");
            }
            Ok(ImageSource::Url(url))
        }
        other => error(format!("Unsupported Anthropic image.source.type: {other}")),
    }
}

fn parse_tool_use(item: &Map<String, Value>) -> Result<Part, TransformError> {
    allowed(
        item,
        &["type", "id", "name", "input", "cache_control"],
        "Anthropic tool_use",
    )?;
    Ok(Part::ToolCall {
        id: string(item.get("id"), "tool_use.id")?,
        name: string(item.get("name"), "tool_use.name")?,
        namespace: None,
        kind: ToolKind::Function,
        input: item
            .get("input")
            .cloned()
            .ok_or_else(|| TransformError("Anthropic tool_use.input is required".to_string()))?,
    })
}

fn parse_tool_result(
    item: &Map<String, Value>,
    transport: Option<&ReasoningTransport>,
    tools: &[Tool],
) -> Result<Part, TransformError> {
    allowed(
        item,
        &[
            "type",
            "tool_use_id",
            "content",
            "is_error",
            "cache_control",
        ],
        "Anthropic tool_result",
    )?;
    let content = match item.get("content") {
        None => vec![],
        Some(value) => parse_anthropic_content(value, Role::User, transport, tools)?,
    };
    if content.iter().any(|part| {
        !matches!(
            part,
            Part::Text(_) | Part::Image(_) | Part::Document(_) | Part::ToolReference(_)
        )
    }) {
        return error("Anthropic tool_result.content must contain only text, images, documents or tool references");
    }
    Ok(Part::ToolResult {
        id: string(item.get("tool_use_id"), "tool_result.tool_use_id")?,
        kind: ToolKind::Function,
        content,
        is_error: optional_bool(item, "is_error")?,
    })
}

fn parse_thinking(
    item: &Map<String, Value>,
    role: Role,
    transport: Option<&ReasoningTransport>,
) -> Result<Part, TransformError> {
    if role != Role::Assistant {
        return error("Anthropic thinking is only valid in assistant messages");
    }
    let transport = transport.ok_or_else(|| {
        TransformError("Reasoning continuation transport is required".to_string())
    })?;
    if item.get("type").and_then(Value::as_str) == Some("redacted_thinking") {
        allowed(item, &["type", "data"], "Anthropic redacted_thinking")?;
        let data = string(item.get("data"), "redacted_thinking.data")?;
        return if ReasoningTransport::is_continuation(&data) {
            transport.from_continuation(data)
        } else {
            transport.from_redacted(data)
        }
        .map(Part::Reasoning);
    }
    allowed(
        item,
        &["type", "thinking", "signature"],
        "Anthropic thinking",
    )?;
    let signature = optional_string(item, "signature", "Anthropic thinking")?;
    if let Some(continuation) = signature
        .as_ref()
        .filter(|value| ReasoningTransport::is_continuation(value))
    {
        let reasoning = transport.from_continuation(continuation.clone())?;
        if reasoning.gemini_turn.is_some() {
            if item.get("thinking").and_then(Value::as_str) != Some("") {
                return error("Gemini 不透明续接不能包含可见思考文本");
            }
        } else {
            string(item.get("thinking"), "thinking.thinking")?;
        }
        return Ok(Part::Reasoning(reasoning));
    }
    let text = string(item.get("thinking"), "thinking.thinking")?;
    transport
        .from_anthropic_thinking(text, signature)
        .map(Part::Reasoning)
}
