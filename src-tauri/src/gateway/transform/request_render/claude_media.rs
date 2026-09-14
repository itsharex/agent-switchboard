//! Render Claude media without discarding document context or tool schemas.

use super::*;

pub(super) fn reference_text(tool: &Tool) -> String {
    json!({"type":"tool_definition", "name":tool.name, "description":tool.description, "parameters":tool.input_schema}).to_string()
}

pub(super) fn document_text(document: &Document) -> String {
    let mut text = Vec::new();
    if let Some(title) = &document.title {
        text.push(format!("Document: {title}"));
    }
    if let Some(context) = &document.context {
        text.push(context.clone());
    }
    if let DocumentSource::Text(data) = &document.source {
        text.push(data.clone());
    }
    text.join("\n\n")
}

pub(super) fn render_document(
    document: &Document,
    target: UpstreamProtocol,
) -> Result<Vec<Value>, TransformError> {
    if target == UpstreamProtocol::AnthropicMessages {
        let source = match &document.source {
            DocumentSource::Text(data) => {
                json!({"type":"text", "media_type":"text/plain", "data":data})
            }
            DocumentSource::Pdf(data) => {
                json!({"type":"base64", "media_type":"application/pdf", "data":data})
            }
            DocumentSource::Url(url) => json!({"type":"url", "url":url}),
        };
        let mut block = json!({"type":"document", "source":source});
        if let Some(title) = &document.title {
            block["title"] = json!(title);
        }
        if let Some(context) = &document.context {
            block["context"] = json!(context);
        }
        return Ok(vec![block]);
    }
    let text_type = if target == UpstreamProtocol::Responses {
        "input_text"
    } else {
        "text"
    };
    let mut parts = Vec::new();
    let text = document_text(document);
    if !text.is_empty() {
        parts.push(json!({"type":text_type, "text":text}));
    }
    let filename = document
        .filename
        .as_deref()
        .or(document.title.as_deref())
        .unwrap_or("document.pdf");
    match (&document.source, target) {
        (DocumentSource::Text(_), _) => {},
        (DocumentSource::Pdf(data), UpstreamProtocol::Responses) => parts.push(json!({"type":"input_file", "filename":filename, "file_data":format!("data:application/pdf;base64,{data}")})),
        (DocumentSource::Pdf(data), _) => parts.push(json!({"type":"file", "file":{"filename":filename, "file_data":format!("data:application/pdf;base64,{data}")}})),
        (DocumentSource::Url(url), UpstreamProtocol::Responses) => parts.push(json!({"type":"input_file", "file_url":url})),
        (DocumentSource::Url(_), _) => return error("Chat Completions 不支持文档 URL；请使用 PDF 数据或 Responses/Anthropic 上游"),
    }
    Ok(parts)
}
