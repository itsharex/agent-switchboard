//! Content-part rendering shared by the request renderers.

use super::*;

/// Replays one reasoning trace to an Anthropic upstream in the only shape that
/// backend accepts: its own opaque block, or a thinking block carrying the text
/// and, when the upstream issued one, its signature.
fn replay_anthropic_reasoning(reasoning: &Reasoning) -> Result<Value, TransformError> {
    if let Some(redacted) = &reasoning.redacted {
        return Ok(json!({ "type": "redacted_thinking", "data": redacted }));
    }
    if reasoning.content.is_empty() {
        return error("该推理没有可发送给 Anthropic 上游的可读内容");
    }
    let mut block = json!({ "type": "thinking", "thinking": reasoning.content });
    if let Some(signature) = &reasoning.signature {
        block["signature"] = Value::String(signature.clone());
    }
    Ok(block)
}

pub(super) fn render_chat_content(parts: &[Part]) -> Result<Value, TransformError> {
    if parts.iter().all(|part| matches!(part, Part::Text(_))) {
        return Ok(Value::String(
            parts
                .iter()
                .filter_map(|part| match part {
                    Part::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        ));
    }
    let mut values = Vec::new();
    for part in parts {
        match part {
            Part::Text(text) => values.push(json!({ "type": "text", "text": text })),
            Part::Image(source) => values.push(json!({
                "type": "image_url",
                "image_url": { "url": image_url(source) },
            })),
            Part::ToolCall { .. } | Part::ToolResult { .. } => {
                return error("工具内容不能嵌入 Chat 文本内容数组");
            }
            Part::Reasoning(_) => return error("推理内容不能嵌入 Chat 文本内容数组"),
            Part::Document(document) => values.extend(super::claude_media::render_document(
                document,
                UpstreamProtocol::ChatCompletions,
            )?),
            Part::File {
                file_id,
                file_data,
                filename,
                ..
            } => {
                // Chat Completions resolves hosted ids and inline data URLs;
                // plain https references have no Chat file representation.
                if file_id.is_none() && file_data.is_none() {
                    return error("Chat Completions 的文件输入需要 file_id 或 file_data");
                }
                let mut file = Map::new();
                if let Some(file_id) = file_id {
                    file.insert("file_id".into(), json!(file_id));
                }
                if let Some(file_data) = file_data {
                    file.insert("file_data".into(), json!(file_data));
                }
                if let Some(filename) = filename {
                    file.insert("filename".into(), json!(filename));
                }
                values.push(json!({ "type": "file", "file": file }));
            }
            Part::Audio { data, format } => values.push(json!({
                "type": "input_audio",
                "input_audio": { "data": data, "format": format },
            })),
            Part::ToolReference(tool) => values
                .push(json!({"type":"text", "text": super::claude_media::reference_text(tool)})),
        }
    }
    Ok(Value::Array(values))
}

pub(super) fn render_anthropic_content(parts: &[Part]) -> Result<Value, TransformError> {
    let mut values = Vec::new();
    for part in parts {
        match part {
            Part::Text(text) => values.push(json!({ "type": "text", "text": text })),
            Part::Image(ImageSource::Data { media_type, data }) => values.push(json!({
                "type": "image",
                "source": { "type": "base64", "media_type": media_type, "data": data },
            })),
            Part::Image(ImageSource::Url(url)) => values.push(json!({"type":"image", "source":{"type":"url", "url":url}})),
            Part::ToolCall {
                id,
                name,
                namespace,
                kind,
                input,
            } => values.push(json!({
                "type": "tool_use",
                "id": id,
                "name": render_target_name(
                    UpstreamProtocol::AnthropicMessages,
                    namespace.as_deref(),
                    name,
                    *kind,
                )?,
                "input": if *kind == ToolKind::Custom { json!({ "input": input.as_str().ok_or_else(|| TransformError("custom 工具输入必须是字符串".to_string()))? }) } else { input.clone() },
            })),
            Part::ToolResult {
                id,
                content,
                is_error,
                ..
            } => values.push(json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": render_anthropic_content(content)?,
                "is_error": is_error,
            })),
            Part::Reasoning(reasoning) => values.push(replay_anthropic_reasoning(reasoning)?),
            Part::Document(document) => values.extend(super::claude_media::render_document(document, UpstreamProtocol::AnthropicMessages)?),
            Part::File { file_id, file_data, file_url, filename } => {
                values.push(render_anthropic_file(file_id.as_deref(), file_data.as_deref(), file_url.as_deref(), filename.as_deref())?);
            }
            Part::Audio { .. } => return error("Anthropic Messages 不支持音频输入"),
            Part::ToolReference(tool) => values.push(json!({"type":"tool_reference", "tool_name": tool.name})),
        }
    }
    Ok(Value::Array(values))
}

pub(super) fn render_responses_content(parts: &[Part]) -> Result<Value, TransformError> {
    let mut values = Vec::new();
    for part in parts {
        match part {
            Part::Text(text) => values.push(json!({ "type": "input_text", "text": text })),
            Part::Image(source) => values.push(json!({
                "type": "input_image",
                "image_url": image_url(source),
            })),
            Part::ToolCall { .. } | Part::ToolResult { .. } => {
                return error("工具内容不能嵌入 Responses message content");
            }
            Part::Reasoning(_) => return error("推理内容不能嵌入 Responses message content"),
            Part::Document(document) => values.extend(super::claude_media::render_document(
                document,
                UpstreamProtocol::Responses,
            )?),
            Part::File {
                file_id,
                file_data,
                file_url,
                filename,
            } => {
                let mut file = Map::new();
                if let Some(file_id) = file_id {
                    file.insert("file_id".into(), json!(file_id));
                }
                if let Some(file_data) = file_data {
                    file.insert("file_data".into(), json!(file_data));
                }
                if let Some(file_url) = file_url {
                    file.insert("file_url".into(), json!(file_url));
                }
                if let Some(filename) = filename {
                    file.insert("filename".into(), json!(filename));
                }
                values.push(json!({ "type": "input_file", "input_file": file }));
            }
            Part::Audio { data, format } => values.push(json!({
                "type": "input_audio",
                "input_audio": { "data": data, "format": format },
            })),
            Part::ToolReference(tool) => values.push(
                json!({"type":"input_text", "text": super::claude_media::reference_text(tool)}),
            ),
        }
    }
    Ok(Value::Array(values))
}

/// Responses `input_file` → Anthropic document block: an https reference
/// becomes a URL source, a data URL becomes a base64 source, and the file
/// name becomes the document title. Hosted ids have no Anthropic meaning.
fn render_anthropic_file(
    file_id: Option<&str>,
    file_data: Option<&str>,
    file_url: Option<&str>,
    filename: Option<&str>,
) -> Result<Value, TransformError> {
    if let Some(file_id) = file_id {
        if file_data.is_none() && file_url.is_none() {
            return error(format!(
                "Anthropic Messages 无法解析仅携带 file_id 的文件输入：{file_id}"
            ));
        }
    }
    let mut block = if let Some(file_url) =
        file_url.filter(|url| url.starts_with("http://") || url.starts_with("https://"))
    {
        json!({ "type": "document", "source": { "type": "url", "url": file_url } })
    } else {
        let Some(file_data) = file_data else {
            return error("Anthropic Messages 的文件输入需要 file_url 或 file_data");
        };
        let Some((meta, data)) = file_data
            .strip_prefix("data:")
            .and_then(|rest| rest.split_once(','))
        else {
            return error("input_file.file_data 必须是 data URL");
        };
        if data.is_empty() {
            return error("input_file.file_data 的 base64 载荷为空");
        }
        let media_type = meta.split(';').next().unwrap_or("application/pdf");
        json!({
            "type": "document",
            "source": { "type": "base64", "media_type": media_type, "data": data },
        })
    };
    if let Some(filename) = filename.filter(|name| !name.is_empty()) {
        block["title"] = json!(filename);
    }
    Ok(block)
}

pub(super) fn image_url(source: &ImageSource) -> String {
    match source {
        ImageSource::Data { media_type, data } => format!("data:{media_type};base64,{data}"),
        ImageSource::Url(url) => url.clone(),
    }
}

pub(super) fn text_only(parts: &[Part], context: &str) -> Result<String, TransformError> {
    let mut text = String::new();
    for part in parts {
        let Part::Text(value) = part else {
            return error(format!("{context} 只能包含文本"));
        };
        text.push_str(value);
    }
    Ok(text)
}
