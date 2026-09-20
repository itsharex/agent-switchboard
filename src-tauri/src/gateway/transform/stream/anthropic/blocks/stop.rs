use super::*;

impl AnthropicToResponses {
    pub(in super::super) fn block_stop(
        &mut self, frame: &Frame, output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "content_block_stop")?;
        let map = object(&value, "content_block_stop")?;
        allowed(map, &["type", "index"], "content_block_stop")?;
        if required_string(map, "type", "content_block_stop")? != "content_block_stop" {
            return Err(TransformError("content_block_stop.type 无效".into()));
        }
        let source_index = required_index(map, "content_block_stop")?;
        let block = self.blocks.get_mut(&source_index).ok_or_else(|| {
            TransformError("content_block_stop 找不到起始 block".into())
        })?;
        if block.stopped() {
            return Err(TransformError("Anthropic SSE 重复 content_block_stop".into()));
        }
        let reasoning = close_block(block, self.reasoning_transport.as_ref())?;
        if let Some(reasoning) = reasoning {
            let index = self.output_index(source_index);
            release_reasoning(output, self.id.as_deref().expect("started"), index, &reasoning);
        }
        Ok(())
    }
}

fn close_block(block: &mut Block, transport: Option<&ReasoningTransport>) -> Result<Option<Reasoning>, TransformError> {
    let reasoning = match block {
        Block::Thinking { text, signature, reasoning, .. } if !text.is_empty() => {
            let transport = transport.ok_or_else(|| TransformError("当前转换缺少本机推理续接通道".into()))?;
            let sealed = transport.from_anthropic_thinking(std::mem::take(text), signature.take())?;
            *reasoning = Some(sealed.clone());
            Some(sealed)
        }
        Block::Redacted { data, reasoning, .. } => {
            let transport = transport.ok_or_else(|| TransformError("当前转换缺少本机推理续接通道".into()))?;
            let sealed = transport.from_redacted(data.clone())?;
            *reasoning = Some(sealed.clone());
            Some(sealed)
        }
        _ => None,
    };
    match block {
        Block::Text { stopped, .. } | Block::Thinking { stopped, .. }
        | Block::Redacted { stopped, .. } | Block::Tool { stopped, .. } => *stopped = true,
    }
    Ok(reasoning)
}
