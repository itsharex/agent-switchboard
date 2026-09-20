//! Validate block envelopes before dispatching typed content handlers.
use super::*;

mod delta;
mod start;
mod stop;

impl AnthropicToResponses {
    pub(super) fn block_start(
        &mut self, frame: &Frame, output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        self.require_started()?;
        let value = json_data(frame, "content_block_start")?;
        let map = block_envelope(&value, "content_block_start", "content_block")?;
        let index = required_index(map, "content_block_start")?;
        if self.blocks.contains_key(&index) {
            return Err(TransformError("Anthropic SSE 重复 content block 索引".into()));
        }
        let block = object(map.get("content_block").ok_or_else(|| {
            TransformError("content_block_start 缺少 content_block".into())
        })?, "content_block")?;
        let kind = required_string(block, "type", "content_block")?;
        let block = match kind.as_str() {
            "text" => self.start_text(index, block, output)?,
            "tool_use" => self.start_tool(index, block, output)?,
            "thinking" => start::thinking(block)?,
            "redacted_thinking" => start::redacted(block)?,
            other => return Err(TransformError(format!("Anthropic SSE content block {other} 不支持转换"))),
        };
        self.blocks.insert(index, block);
        Ok(())
    }

    pub(super) fn block_delta(
        &mut self, frame: &Frame, output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        self.require_started()?;
        let value = json_data(frame, "content_block_delta")?;
        let map = block_envelope(&value, "content_block_delta", "delta")?;
        let source_index = required_index(map, "content_block_delta")?;
        let delta = object(map.get("delta").ok_or_else(|| {
            TransformError("content_block_delta 缺少 delta".into())
        })?, "delta")?;
        let index = self.output_indices.get(&source_index).copied();
        let block = self.blocks.get_mut(&source_index).ok_or_else(|| {
            TransformError("content_block_delta 找不到起始 block".into())
        })?;
        if block.stopped() {
            return Err(TransformError("content block 已结束后继续发送 delta".into()));
        }
        delta::apply(block, index, delta, output)
    }
}

fn block_envelope<'a>(
    value: &'a Value, event: &str, payload: &str,
) -> Result<&'a Map<String, Value>, TransformError> {
    let map = object(value, event)?;
    allowed(map, &["type", "index", payload], event)?;
    if required_string(map, "type", event)? != event {
        return Err(TransformError(format!("{event}.type 无效")));
    }
    Ok(map)
}
