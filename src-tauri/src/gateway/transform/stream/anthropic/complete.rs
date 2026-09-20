//! Terminal snapshots preserve exactly the item identities emitted during streaming.

use super::*;

impl AnthropicToResponses {
    pub(super) fn complete(
        &mut self, frame: &Frame, output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        self.require_started()?;
        let value = json_data(frame, "message_stop")?;
        let map = object(&value, "message_stop")?;
        allowed(map, &["type"], "message_stop")?;
        if required_string(map, "type", "message_stop")? != "message_stop" {
            return Err(TransformError("message_stop.type 无效".to_string()));
        }
        let stop = match self.stop {
            StopReasonState::Unset => return Err(TransformError("message_stop 缺少 stop_reason".into())),
            StopReasonState::Value(reason) => reason,
        };
        let id = self.id.as_deref().expect("started");
        let model = self.model.as_deref().expect("started");
        let status = responses_status(stop);
        let mut items = BTreeMap::new();
        for (source_index, block) in &self.blocks {
            if !block.stopped() {
                return Err(TransformError("message_stop 前存在未结束的 content block".into()));
            }
            if let Some(index) = self.output_indices.get(source_index) {
                let item = completed_block(output, id, *index, block, status)?;
                items.insert(*index, item);
            }
        }
        output::terminal(output, id, model, stop, &self.usage, items.into_values().collect());
        self.completed = true;
        Ok(())
    }

    pub(super) fn require_started(&self) -> Result<(), TransformError> {
        if self.started { Ok(()) } else {
            Err(TransformError("Anthropic SSE 在 message_start 前发送内容".to_string()))
        }
    }
}

fn completed_block(
    output: &mut Vec<u8>, id: &str, index: u64, block: &Block, status: &str,
) -> Result<Value, TransformError> {
    match block {
        Block::Text { text, .. } => Ok(output::text_done(output, id, index, text, status)),
        Block::Thinking { reasoning, .. } | Block::Redacted { reasoning, .. } => {
            Ok(responses_reasoning_item(id, index, reasoning.as_ref().expect("emitted reasoning")))
        }
        Block::Tool { id, name, namespace, kind, arguments, .. } => {
            let item = responses_tool_call_item(id, name, namespace.as_deref(), *kind, arguments, status)?;
            output::tool_done(output, index, *kind, &item);
            Ok(item)
        }
    }
}

pub(super) fn release_reasoning(
    output: &mut Vec<u8>, id: &str, output_index: u64, reasoning: &Reasoning,
) {
    let item = responses_reasoning_item(id, output_index, reasoning);
    for event in ["response.output_item.added", "response.output_item.done"] {
        append_event(output, event, json!({"type":event,"output_index":output_index,"item":item}));
    }
}

pub(super) fn validate_ping(frame: &Frame) -> Result<(), TransformError> {
    let value = json_data(frame, "ping")?;
    let map = object(&value, "ping")?;
    allowed(map, &["type"], "ping")?;
    if required_string(map, "type", "ping")? == "ping" { Ok(()) } else {
        Err(TransformError("ping.type 无效".to_string()))
    }
}
