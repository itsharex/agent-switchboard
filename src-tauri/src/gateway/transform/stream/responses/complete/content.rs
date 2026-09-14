//! Content-part completion and text snapshot consistency.

use super::*;

impl ResponsesToAnthropic {
    pub(in crate::gateway::transform::stream::responses) fn content_part_done(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "response.content_part.done")?;
        let map = object(&value, "response.content_part.done")?;
        allowed(
            map,
            &[
                "type",
                "sequence_number",
                "output_index",
                "content_index",
                "item_id",
                "part",
            ],
            "response.content_part.done",
        )?;
        if required_string(map, "type", "response.content_part.done")?
            != "response.content_part.done"
        {
            return Err(TransformError(
                "response.content_part.done 字段无效".to_string(),
            ));
        }
        let output_index = required_index(map, "output_index")?;
        let content_part_index = required_index(map, "content_index")?;
        let incoming_item_id = event_item_id(map, "response.content_part.done")?;
        let part = object(
            map.get("part").ok_or_else(|| {
                TransformError("response.content_part.done 缺少 part".to_string())
            })?,
            "response.content_part.done.part",
        )?;
        let part_type = required_string(part, "type", "Responses output content part")?;
        let (final_text, part_kind) = match part_type.as_str() {
            "output_text" => {
                allowed(
                    part,
                    &["type", "text", "annotations"],
                    "Responses output text part",
                )?;
                (
                    string_value(part, "text", "Responses output text part")?,
                    TextPartKind::Output,
                )
            }
            "refusal" => {
                allowed(part, &["type", "refusal"], "Responses refusal part")?;
                (
                    string_value(part, "refusal", "Responses refusal part")?,
                    TextPartKind::Refusal,
                )
            }
            _ => {
                return Err(TransformError(
                    "Responses content part done 必须是 output_text 或 refusal".to_string(),
                ))
            }
        };
        self.apply_content_part_done(
            output_index,
            content_part_index,
            incoming_item_id,
            final_text,
            part_kind,
            output,
        )
    }

    fn apply_content_part_done(
        &mut self,
        output_index: u64,
        content_part_index: u64,
        incoming_item_id: Option<String>,
        final_text: String,
        part_kind: TextPartKind,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let item = self.items.get_mut(&output_index).ok_or_else(|| {
            TransformError("Responses content part done 找不到 output item".to_string())
        })?;
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
            return Err(TransformError(
                "Responses content part done 找不到文本 output item".to_string(),
            ));
        };
        merge_event_item_id(item_id, incoming_item_id, "response.content_part.done")?;
        super::super::events::ensure_text_part_kind(
            part_kinds,
            content_part_index,
            part_kind,
            "response.content_part.done",
        )?;
        if part_done.contains(&content_part_index) {
            if parts.get(&content_part_index).map(String::as_str) != Some(final_text.as_str()) {
                return Err(TransformError(
                    "Responses 重复 content part done 内容不一致".to_string(),
                ));
            }
            return Ok(());
        }
        if *closed {
            return Err(TransformError(
                "Responses content part done 出现在文本块完成后".to_string(),
            ));
        }
        let suffix = replace_text_part(
            parts,
            text,
            content_part_index,
            &final_text,
            "Responses content part done",
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
}
