//! Reconcile terminal items against the exact streamed identities and payloads.

use super::*;

impl ResponsesToAnthropic {
    pub(in crate::gateway::transform::stream::responses) fn finish_existing(
        &mut self,
        output_index: u64,
        expected: &Value,
        terminal: ResponsesTerminal,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let item = object(expected, "Responses terminal output item")?;
        terminal.validate_item(item)?;
        let state = self
            .items
            .get_mut(&output_index)
            .expect("existing output item");
        finish_item(state, item, output, self.reasoning_transport.as_ref())
    }
}

pub(super) fn finish_item(
    state: &mut Item,
    item: &Map<String, Value>,
    output: &mut Vec<u8>,
    transport: Option<&ReasoningTransport>,
) -> Result<(), TransformError> {
    match state {
        Item::CustomTool { .. } => finish_custom_item(state, item, output),
        Item::Tool { .. } => finish_function_item(state, item, output),
        Item::Text { .. } => finish_message_item(state, item, output),
        Item::ToolSearch { .. } => finish_tool_search_item(state, item, output),
        Item::Reasoning { .. } => finish_reasoning_item(state, item, output, transport),
    }
}
