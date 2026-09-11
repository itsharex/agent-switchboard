use super::*;

pub(super) fn responses_reasoning_text(
    item: &Map<String, Value>,
) -> Result<String, TransformError> {
    let mut text = String::new();
    if let Some(summary) = item.get("summary") {
        for (index, value) in array(summary, "reasoning.summary")?.iter().enumerate() {
            let part = object(value, &format!("reasoning.summary[{index}]"))?;
            allowed(part, &["type", "text"], "Responses reasoning summary")?;
            if string(part.get("type"), "reasoning.summary.type")? != "summary_text" {
                return error("Responses reasoning.summary.type 必须是 summary_text");
            }
            text.push_str(&string(part.get("text"), "reasoning.summary.text")?);
        }
    }
    if let Some(content) = item.get("content") {
        for (index, value) in array(content, "reasoning.content")?.iter().enumerate() {
            let part = object(value, &format!("reasoning.content[{index}]"))?;
            allowed(part, &["type", "text"], "Responses reasoning content")?;
            if string(part.get("type"), "reasoning.content.type")? != "reasoning_text" {
                return error("Responses reasoning.content.type 必须是 reasoning_text");
            }
            text.push_str(&string(part.get("text"), "reasoning.content.text")?);
        }
    }
    if text.is_empty() {
        return error("Responses reasoning 缺少 encrypted_content 或可转换的 summary/content");
    }
    Ok(text)
}
