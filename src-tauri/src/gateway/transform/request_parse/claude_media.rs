//! Claude documents and deferred client-tool references.

use super::*;
use base64::Engine;

pub(super) fn parse_document(item: &Map<String, Value>) -> Result<Document, TransformError> {
    allowed(
        item,
        &[
            "type",
            "source",
            "title",
            "context",
            "citations",
            "cache_control",
        ],
        "Claude document",
    )?;
    if let Some(citations) = item.get("citations") {
        let citations = object(citations, "document.citations")?;
        allowed(citations, &["enabled"], "document.citations")?;
        if optional_bool(citations, "enabled")? {
            return error("开启文档引用需要 Anthropic 原生上游");
        }
    }
    let source = object(
        item.get("source")
            .ok_or_else(|| TransformError("document 缺少 source".into()))?,
        "document.source",
    )?;
    let data = match string(source.get("type"), "document.source.type")?.as_str() {
        "base64" => {
            allowed(source, &["type", "media_type", "data"], "document.source")?;
            if source.get("media_type").and_then(Value::as_str) != Some("application/pdf") {
                return error("Claude base64 document 必须是 application/pdf");
            }
            let data = string(source.get("data"), "document.source.data")?;
            base64::engine::general_purpose::STANDARD
                .decode(&data)
                .map_err(|_| TransformError("PDF 数据不是有效 base64".into()))?;
            DocumentSource::Pdf(data)
        }
        "text" => {
            allowed(source, &["type", "media_type", "data"], "document.source")?;
            if source.get("media_type").and_then(Value::as_str) != Some("text/plain") {
                return error("Claude text document 必须是 text/plain");
            }
            DocumentSource::Text(string(source.get("data"), "document.source.data")?)
        }
        "url" => {
            allowed(source, &["type", "url"], "document.source")?;
            let url = string(source.get("url"), "document.source.url")?;
            if !reqwest::Url::parse(&url).is_ok_and(|url| {
                matches!(url.scheme(), "http" | "https")
                    && url.username().is_empty()
                    && url.password().is_none()
            }) {
                return error("document.source.url 必须是不含凭据的 HTTP(S) URL");
            }
            DocumentSource::Url(url)
        }
        _ => return error("该 Claude 文档来源需要原生 Anthropic 上游"),
    };
    Ok(Document {
        source: data,
        title: optional_string(item, "title", "document")?,
        filename: None,
        context: optional_string(item, "context", "document")?,
    })
}

pub(super) fn parse_reference(
    item: &Map<String, Value>,
    tools: &[Tool],
) -> Result<Tool, TransformError> {
    allowed(
        item,
        &["type", "tool_name", "cache_control"],
        "Claude tool_reference",
    )?;
    let name = string(item.get("tool_name"), "tool_reference.tool_name")?;
    tools
        .iter()
        .find(|tool| tool.name == name)
        .cloned()
        .ok_or_else(|| {
            TransformError(format!(
                "tool_reference 引用了本轮目录中不存在的工具：{name}"
            ))
        })
}

pub(super) fn validate_tool_metadata(item: &Map<String, Value>) -> Result<(), TransformError> {
    if item
        .get("type")
        .is_some_and(|v| v.as_str() != Some("custom"))
    {
        return error("Claude 服务端工具需要原生 Anthropic 上游");
    }
    for key in ["defer_loading", "eager_input_streaming"] {
        optional_bool(item, key)?;
    }
    if let Some(callers) = item.get("allowed_callers") {
        if !array(callers, "tool.allowed_callers")?
            .iter()
            .all(|v| v.as_str() == Some("direct"))
        {
            return error("Claude 服务端执行工具不能转换为本机工具调用");
        }
    }
    if let Some(examples) = item.get("input_examples") {
        array(examples, "tool.input_examples")?;
    }
    Ok(())
}

pub(super) fn tool_description(
    item: &Map<String, Value>,
) -> Result<Option<String>, TransformError> {
    let mut description = optional_string(item, "description", "Claude tool")?;
    if let Some(examples) = item
        .get("input_examples")
        .filter(|v| v.as_array().is_some_and(|a| !a.is_empty()))
    {
        description = Some(format!(
            "{}\nInput examples: {}",
            description.unwrap_or_default(),
            examples
        ));
    }
    Ok(description)
}
