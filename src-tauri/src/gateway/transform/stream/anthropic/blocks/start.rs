use super::*;

impl AnthropicToResponses {
    pub(super) fn start_text(
        &mut self, source_index: u64, block: &Map<String, Value>, output: &mut Vec<u8>,
    ) -> Result<Block, TransformError> {
        allowed(block, &["type", "text"], "Anthropic text block")?;
        let text = block_string(block, "text", "Anthropic text block")?;
        let index = self.output_index(source_index);
        let id = self.id.as_deref().expect("started");
        append_event(output, "response.output_item.added", json!({
            "type":"response.output_item.added", "output_index":index,
            "item":{"type":"message", "id":format!("msg_{id}_{index}"),
                "status":"in_progress", "role":"assistant", "content":[]},
        }));
        append_event(output, "response.content_part.added", json!({
            "type":"response.content_part.added", "output_index":index, "content_index":0,
            "part":{"type":"output_text", "text":"", "annotations":[]},
        }));
        if !text.is_empty() {
            append_event(output, "response.output_text.delta", json!({
                "type":"response.output_text.delta", "output_index":index,
                "content_index":0, "delta":text,
            }));
        }
        Ok(Block::Text { text, stopped: false })
    }

    pub(super) fn start_tool(
        &mut self, source_index: u64, block: &Map<String, Value>, output: &mut Vec<u8>,
    ) -> Result<Block, TransformError> {
        allowed(block, &["type", "id", "name", "input"], "Anthropic tool block")?;
        let input = block.get("input").and_then(Value::as_object).ok_or_else(|| {
            TransformError("Anthropic tool block.input 必须是对象".into())
        })?;
        if !input.is_empty() {
            return Err(TransformError("Anthropic 流式 tool_use 初始 input 必须为空对象".into()));
        }
        let id = required_string(block, "id", "Anthropic tool block")?;
        let encoded_name = required_string(block, "name", "Anthropic tool block")?;
        let (namespace, name, kind) = parse_target_name(&encoded_name)?;
        let index = self.output_index(source_index);
        let item = responses_tool_call_item(&id, &name, namespace.as_deref(), kind, "", "in_progress")?;
        append_event(output, "response.output_item.added", json!({
            "type":"response.output_item.added", "output_index":index, "item":item,
        }));
        Ok(Block::Tool { id, name, namespace, kind, arguments: String::new(), stopped: false })
    }
}

pub(super) fn thinking(block: &Map<String, Value>) -> Result<Block, TransformError> {
    allowed(block, &["type", "thinking", "signature"], "Anthropic thinking block")?;
    let signature = match block.get("signature") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if value.is_empty() => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => return Err(TransformError("Anthropic thinking block.signature 必须是字符串".into())),
    };
    let text = block_string(block, "thinking", "Anthropic thinking block")?;
    Ok(Block::Thinking { text, signature, stopped: false, reasoning: None })
}

pub(super) fn redacted(block: &Map<String, Value>) -> Result<Block, TransformError> {
    allowed(block, &["type", "data"], "Anthropic redacted_thinking")?;
    let data = required_string(block, "data", "Anthropic redacted_thinking")?;
    Ok(Block::Redacted { data, stopped: false, reasoning: None })
}

fn block_string(block: &Map<String, Value>, field: &str, context: &str) -> Result<String, TransformError> {
    block.get(field).and_then(Value::as_str).map(str::to_string)
        .ok_or_else(|| TransformError(format!("{context}.{field} 必须是字符串")))
}
