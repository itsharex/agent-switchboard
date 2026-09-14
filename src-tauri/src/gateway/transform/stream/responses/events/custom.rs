//! Custom-tool input events and their JSON-wrapper state.

use super::*;

impl ResponsesToAnthropic {
    pub(in crate::gateway::transform::stream::responses) fn custom_tool_delta(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "response.custom_tool_call_input.delta")?;
        let map = object(&value, "response.custom_tool_call_input.delta")?;
        allowed(
            map,
            &[
                "type",
                "sequence_number",
                "output_index",
                "item_id",
                "call_id",
                "delta",
                "obfuscation",
            ],
            "response.custom_tool_call_input.delta",
        )?;
        if required_string(map, "type", "response.custom_tool_call_input.delta")?
            != "response.custom_tool_call_input.delta"
        {
            return Err(TransformError(
                "response.custom_tool_call_input.delta.type 无效".to_string(),
            ));
        }
        validate_obfuscation(map, "response.custom_tool_call_input.delta")?;
        let output_index = required_index(map, "output_index")?;
        let incoming_item_id = event_item_id(map, "response.custom_tool_call_input.delta")?;
        let delta = required_value_string(
            map.get("delta")
                .ok_or_else(|| TransformError("custom tool delta 缺少 delta".to_string()))?,
            "response.custom_tool_call_input.delta.delta",
        )?;
        let call_id = optional_string(map, "call_id", "response.custom_tool_call_input.delta")?;
        self.apply_custom_tool_delta(output_index, incoming_item_id, delta, call_id, output)
    }

    fn apply_custom_tool_delta(
        &mut self,
        output_index: u64,
        incoming_item_id: Option<String>,
        delta: String,
        call_id: Option<String>,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let item = self.items.get_mut(&output_index).ok_or_else(|| {
            TransformError("Responses custom tool delta 找不到 output item".to_string())
        })?;
        let Item::CustomTool {
            content_index,
            id,
            item_id,
            input,
            encoded_len,
            closed,
            ..
        } = item
        else {
            return Err(TransformError(
                "Responses custom tool delta 指向非 custom 工具".to_string(),
            ));
        };
        if *closed {
            return Err(TransformError(
                "Responses custom tool delta 出现在工具完成后".to_string(),
            ));
        }
        merge_event_item_id(
            item_id,
            incoming_item_id,
            "response.custom_tool_call_input.delta",
        )?;
        if call_id.as_deref().is_some_and(|value| value != id) {
            return Err(TransformError(
                "Responses custom tool delta.call_id 与 output item 不一致".to_string(),
            ));
        }
        input.push_str(&delta);
        let encoded = custom_argument_prefix(input)?;
        let suffix = encoded
            .get(*encoded_len..)
            .ok_or_else(|| TransformError("Responses custom tool delta 顺序无效".to_string()))?
            .to_string();
        *encoded_len = encoded.len();
        if !suffix.is_empty() {
            append_event(
                output,
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": *content_index,
                    "delta": { "type": "input_json_delta", "partial_json": suffix },
                }),
            );
        }
        Ok(())
    }

    pub(in crate::gateway::transform::stream::responses) fn custom_tool_done(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "response.custom_tool_call_input.done")?;
        let map = object(&value, "response.custom_tool_call_input.done")?;
        allowed(
            map,
            &[
                "type",
                "sequence_number",
                "output_index",
                "item_id",
                "call_id",
                "input",
                "obfuscation",
            ],
            "response.custom_tool_call_input.done",
        )?;
        if required_string(map, "type", "response.custom_tool_call_input.done")?
            != "response.custom_tool_call_input.done"
        {
            return Err(TransformError(
                "response.custom_tool_call_input.done.type 无效".to_string(),
            ));
        }
        validate_obfuscation(map, "response.custom_tool_call_input.done")?;
        let output_index = required_index(map, "output_index")?;
        let incoming_item_id = event_item_id(map, "response.custom_tool_call_input.done")?;
        let input = required_value_string(
            map.get("input")
                .ok_or_else(|| TransformError("custom tool done 缺少 input".to_string()))?,
            "response.custom_tool_call_input.done.input",
        )?;
        let call_id = optional_string(map, "call_id", "response.custom_tool_call_input.done")?;
        self.apply_custom_tool_done(output_index, incoming_item_id, input, call_id, output)
    }

    fn apply_custom_tool_done(
        &mut self,
        output_index: u64,
        incoming_item_id: Option<String>,
        input: String,
        call_id: Option<String>,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let item = self.items.get_mut(&output_index).ok_or_else(|| {
            TransformError("Responses custom tool done 找不到 output item".to_string())
        })?;
        let Item::CustomTool {
            content_index,
            id,
            item_id,
            input: current,
            encoded_len,
            closed,
            ..
        } = item
        else {
            return Err(TransformError(
                "Responses custom tool done 指向非 custom 工具".to_string(),
            ));
        };
        merge_event_item_id(
            item_id,
            incoming_item_id,
            "response.custom_tool_call_input.done",
        )?;
        if call_id.as_deref().is_some_and(|value| value != id) {
            return Err(TransformError(
                "Responses custom tool done.call_id 与 output item 不一致".to_string(),
            ));
        }
        if *closed {
            if current != &input {
                return Err(TransformError(
                    "Responses custom input changed after its block was closed".into(),
                ));
            }
            return Ok(());
        }
        let _ = append_suffix(current, &input, "Responses custom tool input")?;
        let encoded = custom_arguments(current)?;
        let suffix = encoded
            .get(*encoded_len..)
            .ok_or_else(|| TransformError("Responses custom tool done 顺序无效".to_string()))?
            .to_string();
        *encoded_len = encoded.len();
        if !suffix.is_empty() {
            append_event(
                output,
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": *content_index,
                    "delta": { "type": "input_json_delta", "partial_json": suffix },
                }),
            );
        }
        if !*closed {
            append_event(
                output,
                "content_block_stop",
                json!({ "type": "content_block_stop", "index": *content_index }),
            );
            *closed = true;
        }
        // The input-done event closes the item payload; output_item.done may
        // arrive later and must be treated as a consistency check only.
        if let Item::CustomTool { item_done, .. } = item {
            *item_done = true;
        }
        Ok(())
    }
}
