//! Text and refusal deltas, snapshots, and part completion.

use super::*;

impl ResponsesToAnthropic {
    pub(in crate::gateway::transform::stream::responses) fn text_delta(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "response.output_text.delta")?;
        let map = object(&value, "response.output_text.delta")?;
        allowed(
            map,
            &[
                "type",
                "sequence_number",
                "output_index",
                "content_index",
                "item_id",
                "delta",
                "logprobs",
                "obfuscation",
            ],
            "response.output_text.delta",
        )?;
        if required_string(map, "type", "response.output_text.delta")?
            != "response.output_text.delta"
        {
            return Err(TransformError(
                "response.output_text.delta 字段无效".to_string(),
            ));
        }
        let output_index = required_index(map, "output_index")?;
        let content_part_index = required_index(map, "content_index")?;
        let incoming_item_id = event_item_id(map, "response.output_text.delta")?;
        validate_optional_array(map, "logprobs", "response.output_text.delta")?;
        validate_obfuscation(map, "response.output_text.delta")?;
        let delta = string_value(map, "delta", "response.output_text.delta")?;
        self.apply_text_delta(
            output_index,
            content_part_index,
            incoming_item_id,
            delta,
            output,
        )
    }

    fn apply_text_delta(
        &mut self,
        output_index: u64,
        content_part_index: u64,
        incoming_item_id: Option<String>,
        delta: String,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let (content_index, suffix) = match self.items.get_mut(&output_index) {
            Some(Item::Text {
                content_index,
                item_id,
                text,
                parts,
                part_kinds,
                part_done,
                closed,
                ..
            }) => {
                if *closed {
                    return Err(TransformError(
                        "Responses text delta 出现在文本完成后".to_string(),
                    ));
                }
                if part_done.contains(&content_part_index) {
                    return Err(TransformError(
                        "Responses text delta 出现在 content part 完成后".to_string(),
                    ));
                }
                merge_event_item_id(item_id, incoming_item_id, "response.output_text.delta")?;
                ensure_text_part_kind(
                    part_kinds,
                    content_part_index,
                    TextPartKind::Output,
                    "response.output_text.delta",
                )?;
                let suffix = append_text_part(
                    parts,
                    text,
                    content_part_index,
                    &delta,
                    "Responses output text delta",
                )?;
                (*content_index, suffix)
            }
            Some(_) => {
                return Err(TransformError(
                    "Responses text delta 指向工具调用".to_string(),
                ))
            }
            None => {
                return Err(TransformError(
                    "Responses text delta 找不到 output item".to_string(),
                ))
            }
        };
        if !suffix.is_empty() {
            append_event(
                output,
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": content_index,
                    "delta": { "type": "text_delta", "text": suffix },
                }),
            );
        }
        Ok(())
    }

    pub(in crate::gateway::transform::stream::responses) fn text_done(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "response.output_text.done")?;
        let map = object(&value, "response.output_text.done")?;
        allowed(
            map,
            &[
                "type",
                "sequence_number",
                "output_index",
                "content_index",
                "item_id",
                "text",
                "logprobs",
                "obfuscation",
            ],
            "response.output_text.done",
        )?;
        if required_string(map, "type", "response.output_text.done")? != "response.output_text.done"
        {
            return Err(TransformError(
                "response.output_text.done 字段无效".to_string(),
            ));
        }
        validate_optional_array(map, "logprobs", "response.output_text.done")?;
        validate_obfuscation(map, "response.output_text.done")?;
        let output_index = required_index(map, "output_index")?;
        let content_part_index = required_index(map, "content_index")?;
        let incoming_item_id = event_item_id(map, "response.output_text.done")?;
        let text = string_value(map, "text", "response.output_text.done")?;
        self.apply_text_done(
            output_index,
            content_part_index,
            incoming_item_id,
            text,
            output,
        )
    }

    fn apply_text_done(
        &mut self,
        output_index: u64,
        content_part_index: u64,
        incoming_item_id: Option<String>,
        text: String,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let item = self
            .items
            .get_mut(&output_index)
            .ok_or_else(|| TransformError("Responses text done 找不到 output item".to_string()))?;
        let Item::Text {
            content_index,
            item_id,
            text: current,
            parts,
            part_kinds,
            part_done,
            closed,
            ..
        } = item
        else {
            return Err(TransformError(
                "Responses text done 指向工具调用".to_string(),
            ));
        };
        merge_event_item_id(item_id, incoming_item_id, "response.output_text.done")?;
        ensure_text_part_kind(
            part_kinds,
            content_part_index,
            TextPartKind::Output,
            "response.output_text.done",
        )?;
        if part_done.contains(&content_part_index) {
            if parts.get(&content_part_index).map(String::as_str) != Some(text.as_str()) {
                return Err(TransformError(
                    "Responses 重复 text done 内容不一致".to_string(),
                ));
            }
            return Ok(());
        }
        if *closed {
            return Err(TransformError(
                "Responses text done 出现在文本块完成后".to_string(),
            ));
        }
        let suffix = replace_text_part(
            parts,
            current,
            content_part_index,
            &text,
            "Responses output text done",
        )?;
        part_done.insert(content_part_index);
        if !suffix.is_empty() {
            append_event(
                output,
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": *content_index,
                    "delta": { "type": "text_delta", "text": suffix },
                }),
            );
        }
        Ok(())
    }

    pub(in crate::gateway::transform::stream::responses) fn refusal_delta(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let event = "response.refusal.delta";
        let value = json_data(frame, event)?;
        let map = object(&value, event)?;
        allowed(
            map,
            &[
                "type",
                "item_id",
                "output_index",
                "content_index",
                "delta",
                "sequence_number",
            ],
            event,
        )?;
        if required_string(map, "type", event)? != event {
            return Err(TransformError(format!("{event}.type 无效")));
        }
        let output_index = required_index(map, "output_index")?;
        let content_part_index = required_index(map, "content_index")?;
        let incoming_item_id = event_item_id(map, event)?;
        let delta = string_value(map, "delta", event)?;
        let item = self
            .items
            .get_mut(&output_index)
            .ok_or_else(|| TransformError(format!("{event} 找不到 output item")))?;
        let Item::Text {
            content_index,
            item_id,
            text,
            parts,
            part_kinds,
            part_done,
            closed,
            ..
        } = item
        else {
            return Err(TransformError(format!("{event} 指向非 message item")));
        };
        merge_event_item_id(item_id, incoming_item_id, event)?;
        if *closed || part_done.contains(&content_part_index) {
            return Err(TransformError(format!("{event} 出现在文本块完成后")));
        }
        ensure_text_part_kind(part_kinds, content_part_index, TextPartKind::Refusal, event)?;
        let suffix = append_text_part(parts, text, content_part_index, &delta, event)?;
        if !suffix.is_empty() {
            append_event(
                output,
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": *content_index,
                    "delta": { "type": "text_delta", "text": suffix },
                }),
            );
        }
        Ok(())
    }

    pub(in crate::gateway::transform::stream::responses) fn refusal_done(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let event = "response.refusal.done";
        let value = json_data(frame, event)?;
        let map = object(&value, event)?;
        allowed(
            map,
            &[
                "type",
                "item_id",
                "output_index",
                "content_index",
                "refusal",
                "sequence_number",
            ],
            event,
        )?;
        if required_string(map, "type", event)? != event {
            return Err(TransformError(format!("{event}.type 无效")));
        }
        let output_index = required_index(map, "output_index")?;
        let content_part_index = required_index(map, "content_index")?;
        let incoming_item_id = event_item_id(map, event)?;
        let refusal = string_value(map, "refusal", event)?;
        let item = self
            .items
            .get_mut(&output_index)
            .ok_or_else(|| TransformError(format!("{event} 找不到 output item")))?;
        let Item::Text {
            content_index,
            item_id,
            text,
            parts,
            part_kinds,
            part_done,
            closed,
            ..
        } = item
        else {
            return Err(TransformError(format!("{event} 指向非 message item")));
        };
        merge_event_item_id(item_id, incoming_item_id, event)?;
        ensure_text_part_kind(part_kinds, content_part_index, TextPartKind::Refusal, event)?;
        if part_done.contains(&content_part_index) {
            if parts.get(&content_part_index).map(String::as_str) != Some(refusal.as_str()) {
                return Err(TransformError(format!("{event} 重复内容不一致")));
            }
            return Ok(());
        }
        if *closed {
            return Err(TransformError(format!("{event} 出现在文本块完成后")));
        }
        let suffix = replace_text_part(parts, text, content_part_index, &refusal, event)?;
        part_done.insert(content_part_index);
        if !suffix.is_empty() {
            append_event(
                output,
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": *content_index,
                    "delta": { "type": "text_delta", "text": suffix },
                }),
            );
        }
        Ok(())
    }
}
