//! Function-tool argument deltas and argument completion.

use super::*;

impl ResponsesToAnthropic {
    pub(in crate::gateway::transform::stream::responses) fn tool_delta(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "response.function_call_arguments.delta")?;
        let map = object(&value, "response.function_call_arguments.delta")?;
        allowed(
            map,
            &[
                "type",
                "sequence_number",
                "output_index",
                "item_id",
                "delta",
                "obfuscation",
            ],
            "response.function_call_arguments.delta",
        )?;
        if required_string(map, "type", "response.function_call_arguments.delta")?
            != "response.function_call_arguments.delta"
        {
            return Err(TransformError(
                "response.function_call_arguments.delta.type 无效".to_string(),
            ));
        }
        let output_index = required_index(map, "output_index")?;
        let incoming_item_id = event_item_id(map, "response.function_call_arguments.delta")?;
        validate_obfuscation(map, "response.function_call_arguments.delta")?;
        let delta = string_value(map, "delta", "response.function_call_arguments.delta")?;
        let content_index = match self.items.get_mut(&output_index) {
            Some(Item::Tool {
                content_index,
                item_id,
                arguments,
                closed,
                ..
            }) => {
                if *closed {
                    return Err(TransformError(
                        "Responses function arguments delta 出现在工具完成后".to_string(),
                    ));
                }
                merge_event_item_id(
                    item_id,
                    incoming_item_id,
                    "response.function_call_arguments.delta",
                )?;
                arguments.push_str(&delta);
                *content_index
            }
            Some(_) => return Err(TransformError("Responses 工具 delta 指向文本".to_string())),
            None => {
                return Err(TransformError(
                    "Responses 工具 delta 找不到 output item".to_string(),
                ))
            }
        };
        if !delta.is_empty() {
            append_event(
                output,
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": content_index,
                    "delta": { "type": "input_json_delta", "partial_json": delta },
                }),
            );
        }
        Ok(())
    }

    pub(in crate::gateway::transform::stream::responses) fn tool_done(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "response.function_call_arguments.done")?;
        let map = object(&value, "response.function_call_arguments.done")?;
        allowed(
            map,
            &[
                "type",
                "sequence_number",
                "output_index",
                "item_id",
                "arguments",
                "obfuscation",
            ],
            "response.function_call_arguments.done",
        )?;
        if required_string(map, "type", "response.function_call_arguments.done")?
            != "response.function_call_arguments.done"
        {
            return Err(TransformError(
                "response.function_call_arguments.done.type 无效".to_string(),
            ));
        }
        validate_obfuscation(map, "response.function_call_arguments.done")?;
        let output_index = required_index(map, "output_index")?;
        let incoming_item_id = event_item_id(map, "response.function_call_arguments.done")?;
        let arguments = string_value(map, "arguments", "response.function_call_arguments.done")?;
        response_lifecycle::function_arguments(&arguments)?;
        let item = self.items.get_mut(&output_index).ok_or_else(|| {
            TransformError("Responses function arguments done 找不到 output item".to_string())
        })?;
        let Item::Tool {
            content_index,
            item_id,
            arguments: current,
            closed,
            ..
        } = item
        else {
            return Err(TransformError(
                "Responses function arguments done 指向非 function 工具".to_string(),
            ));
        };
        merge_event_item_id(
            item_id,
            incoming_item_id,
            "response.function_call_arguments.done",
        )?;
        if *closed {
            if response_lifecycle::function_arguments(current)?
                != response_lifecycle::function_arguments(&arguments)?
            {
                return Err(TransformError(
                    "Responses 重复 function arguments done 内容不一致".to_string(),
                ));
            }
            return Ok(());
        }
        let suffix = argument_suffix(current, &arguments)?;
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
        close_item(item, output);
        if let Item::Tool { item_done, .. } = item {
            *item_done = true;
        }
        Ok(())
    }
}
