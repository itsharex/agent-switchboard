//! Content-part snapshots and their incremental text state.

use super::*;

impl ResponsesToAnthropic {
    pub(in crate::gateway::transform::stream::responses) fn content_part_added(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "response.content_part.added")?;
        let map = object(&value, "response.content_part.added")?;
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
            "response.content_part.added",
        )?;
        if required_string(map, "type", "response.content_part.added")?
            != "response.content_part.added"
        {
            return Err(TransformError(
                "response.content_part.added.type 无效".to_string(),
            ));
        }
        let content_part_index = required_index(map, "content_index")?;
        let output_index = required_index(map, "output_index")?;
        let incoming_item_id = event_item_id(map, "response.content_part.added")?;
        let part = object(
            map.get("part").ok_or_else(|| {
                TransformError("response.content_part.added 缺少 part".to_string())
            })?,
            "response.content_part.added.part",
        )?;
        let part_type = required_string(part, "type", "Responses output content part")?;
        let (initial, part_kind) = match part_type.as_str() {
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
                    "Responses content part 必须是 output_text 或 refusal".to_string(),
                ))
            }
        };
        self.apply_content_part_added(
            output_index,
            content_part_index,
            incoming_item_id,
            initial,
            part_kind,
            output,
        )
    }

    fn apply_content_part_added(
        &mut self,
        output_index: u64,
        content_part_index: u64,
        incoming_item_id: Option<String>,
        initial: String,
        part_kind: TextPartKind,
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
                merge_event_item_id(item_id, incoming_item_id, "response.content_part.added")?;
                if *closed {
                    return Err(TransformError(
                        "Responses content part 出现在文本完成后".to_string(),
                    ));
                }
                if part_done.contains(&content_part_index) {
                    let existing = parts.get(&content_part_index).map(String::as_str);
                    if existing != Some(initial.as_str()) {
                        return Err(TransformError(
                            "Responses 重复 content part added 内容不一致".to_string(),
                        ));
                    }
                    return Ok(());
                }
                ensure_text_part_kind(
                    part_kinds,
                    content_part_index,
                    part_kind,
                    "response.content_part.added",
                )?;
                let suffix = merge_text_part_snapshot(
                    parts,
                    text,
                    content_part_index,
                    &initial,
                    "Responses content part",
                )?;
                (*content_index, suffix)
            }
            Some(_) => {
                return Err(TransformError(
                    "Responses text part 指向工具调用".to_string(),
                ))
            }
            None => {
                return Err(TransformError(
                    "Responses text part 找不到 output item".to_string(),
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
}
