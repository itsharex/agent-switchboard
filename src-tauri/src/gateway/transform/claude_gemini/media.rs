use super::super::{DocumentSource, ImageSource};
use super::*;

pub(super) fn part(part: &Part) -> Result<Vec<Value>, TransformError> {
    Ok(match part {
        Part::Text(text) => vec![json!({"text":text})],
        Part::Image(ImageSource::Data { media_type, data }) => {
            vec![json!({"inlineData":{"mimeType":media_type,"data":data}})]
        }
        Part::Image(ImageSource::Url(url)) => vec![json!({"fileData":{"fileUri":url}})],
        Part::Document(document) => {
            let mut parts = Vec::new();
            let mut context = Vec::new();
            if let Some(title) = &document.title {
                context.push(format!("Document: {title}"));
            }
            if let Some(filename) = &document.filename {
                context.push(format!("Filename: {filename}"));
            }
            if let Some(value) = &document.context {
                context.push(value.clone());
            }
            if !context.is_empty() {
                parts.push(json!({"text":context.join("\n\n")}));
            }
            parts.push(match &document.source {
                DocumentSource::Text(text) => json!({"text":text}),
                DocumentSource::Pdf(data) => {
                    json!({"inlineData":{"mimeType":"application/pdf","data":data}})
                }
                DocumentSource::Url(url) => {
                    json!({"fileData":{"mimeType":"application/pdf","fileUri":url}})
                }
            });
            parts
        }
        Part::ToolReference(tool) => vec![
            json!({"text":json!({"type":"tool_definition","name":tool.name,"description":tool.description,"parameters":tool.input_schema}).to_string()}),
        ],
        _ => return error("Gemini 内容位置不支持工具或不透明推理"),
    })
}

pub(super) fn tool_result(
    name: &str,
    id: Option<&str>,
    content: &[Part],
    is_error: bool,
    model: &str,
) -> Result<Vec<Value>, TransformError> {
    let mut text = Vec::new();
    let mut media = Vec::new();
    for item in content {
        for part in part(item)? {
            if let Some(value) = part.get("text").and_then(Value::as_str) {
                text.push(value.to_string());
            } else {
                media.push(part);
            }
        }
    }
    let response = if is_error {
        json!({"error":text.join("\n")})
    } else {
        json!({"output":text.join("\n")})
    };
    let mut function = json!({"name":name,"response":response});
    if let Some(id) = id {
        function["id"] = json!(id);
    }
    let mut output = Vec::new();
    if model.trim_start_matches("models/").starts_with("gemini-3") && !media.is_empty() {
        function["parts"] = json!(media);
    } else {
        output = media;
    }
    output.insert(0, json!({"functionResponse":function}));
    Ok(output)
}
