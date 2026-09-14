//! Message, content-part, and tool-call parsing shared across protocols.

use super::*;

pub(super) fn parse_role(value: &str) -> Result<Role, TransformError> {
    match value {
        "system" => Ok(Role::System),
        "developer" => Ok(Role::Developer),
        "user" | "tool" => Ok(Role::User),
        "assistant" => Ok(Role::Assistant),
        other => error(format!("消息角色 {other} 不支持跨协议转换")),
    }
}

pub(super) fn append_assistant_parts(messages: &mut Vec<Message>, parts: Vec<Part>) {
    if let Some(message) = messages
        .last_mut()
        .filter(|message| message.role == Role::Assistant)
    {
        message.parts.extend(parts);
    } else {
        messages.push(Message {
            role: Role::Assistant,
            parts,
        });
    }
}

pub(super) fn parse_response_content(value: &Value) -> Result<Vec<Part>, TransformError> {
    if let Some(text) = value.as_str() {
        return Ok(vec![Part::Text(text.to_string())]);
    }
    let mut parts = Vec::new();
    for value in array(value, "Responses content")? {
        let item = object(value, "Responses content item")?;
        let kind = string(item.get("type"), "content.type")?;
        match kind.as_str() {
            "input_text" | "output_text" => {
                allowed(item, &["type", "text", "annotations"], "text content")?;
                if let Some(annotations) = item.get("annotations") {
                    if !annotations.is_array() {
                        return error("text content.annotations 必须是数组");
                    }
                    if !annotations
                        .as_array()
                        .is_some_and(|annotations| annotations.is_empty())
                    {
                        return error("Responses text annotations 无法安全转换");
                    }
                }
                parts.push(Part::Text(string(item.get("text"), "content.text")?));
            }
            "input_image" => {
                allowed(item, &["type", "image_url"], "image content")?;
                parts.push(Part::Image(parse_image_url(
                    item.get("image_url")
                        .ok_or_else(|| TransformError("input_image 缺少 image_url".to_string()))?,
                )?));
            }
            other => return error(format!("Responses content.type {other} 不支持跨协议转换")),
        }
    }
    Ok(parts)
}

pub(super) fn parse_chat_content(value: &Value) -> Result<Vec<Part>, TransformError> {
    if let Some(text) = value.as_str() {
        return Ok(vec![Part::Text(text.to_string())]);
    }
    let mut parts = Vec::new();
    for value in array(value, "Chat content")? {
        let item = object(value, "Chat content item")?;
        let kind = string(item.get("type"), "content.type")?;
        match kind.as_str() {
            "text" => {
                allowed(item, &["type", "text"], "Chat text content")?;
                parts.push(Part::Text(string(item.get("text"), "content.text")?));
            }
            "image_url" => {
                allowed(item, &["type", "image_url"], "Chat image content")?;
                parts.push(Part::Image(parse_image_url(
                    item.get("image_url")
                        .ok_or_else(|| TransformError("image_url 缺少值".to_string()))?,
                )?));
            }
            other => return error(format!("Chat content.type {other} 不支持跨协议转换")),
        }
    }
    Ok(parts)
}

pub(super) fn parse_image_url(value: &Value) -> Result<ImageSource, TransformError> {
    let url = match value {
        Value::String(url) => url.clone(),
        Value::Object(map) => {
            allowed(map, &["url", "detail"], "image_url")?;
            string(map.get("url"), "image_url.url")?
        }
        _ => return error("image_url 必须是字符串或对象"),
    };
    if let Some((prefix, data)) = url.split_once(",") {
        let Some(media) = prefix
            .strip_prefix("data:")
            .and_then(|prefix| prefix.strip_suffix(";base64"))
        else {
            return error("仅支持 data:<media>;base64 图片 URL");
        };
        return Ok(ImageSource::Data {
            media_type: media.to_string(),
            data: data.to_string(),
        });
    }
    Ok(ImageSource::Url(url))
}

pub(super) fn parse_chat_tool_calls(value: &Value) -> Result<Vec<Part>, TransformError> {
    let mut parts = Vec::new();
    for value in array(value, "tool_calls")? {
        let item = object(value, "tool_call")?;
        allowed(item, &["id", "type", "function"], "tool_call")?;
        if string(item.get("type"), "tool_call.type")? != "function" {
            return error("仅支持 function tool_call");
        }
        let function = object(
            item.get("function")
                .ok_or_else(|| TransformError("tool_call 缺少 function".to_string()))?,
            "tool_call.function",
        )?;
        allowed(function, &["name", "arguments"], "tool_call.function")?;
        let arguments = string(function.get("arguments"), "tool_call.function.arguments")?;
        let input = serde_json::from_str(&arguments)
            .map_err(|_| TransformError("tool_call.arguments 必须是 JSON 对象".to_string()))?;
        parts.push(Part::ToolCall {
            id: string(item.get("id"), "tool_call.id")?,
            name: string(function.get("name"), "tool_call.function.name")?,
            namespace: None,
            kind: ToolKind::Function,
            input,
        });
    }
    Ok(parts)
}
