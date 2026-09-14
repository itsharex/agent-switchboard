//! Streaming item dispatch and shared event invariants.

mod content;
mod custom;
mod items;
mod text;
mod tools;

use super::*;

impl ResponsesToAnthropic {
    pub(super) fn item_added(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        self.require_started()?;
        let value = json_data(frame, "response.output_item.added")?;
        let map = object(&value, "response.output_item.added")?;
        allowed(
            map,
            &["type", "sequence_number", "output_index", "item"],
            "response.output_item.added",
        )?;
        if required_string(map, "type", "response.output_item.added")?
            != "response.output_item.added"
        {
            return Err(TransformError(
                "response.output_item.added.type 无效".to_string(),
            ));
        }
        let output_index = required_index(map, "output_index")?;
        if self.items.contains_key(&output_index) {
            return Err(TransformError(
                "Responses SSE 重复 output_index".to_string(),
            ));
        }
        let item = object(
            map.get("item").ok_or_else(|| {
                TransformError("response.output_item.added 缺少 item".to_string())
            })?,
            "response.output_item.added.item",
        )?;
        let content_index = output_index;
        match required_string(item, "type", "response output item")?.as_str() {
            "message" => self.add_message_item(output_index, content_index, item, output)?,
            "reasoning" => self.add_reasoning_item(output_index, content_index, item)?,
            "function_call" => self.add_function_item(output_index, content_index, item, output)?,
            "custom_tool_call" => {
                self.add_custom_item(output_index, content_index, item, output)?
            }
            "tool_search_call" => {
                self.add_tool_search_item(output_index, content_index, item, output)?
            }
            other => {
                return Err(TransformError(format!(
                    "Responses output item {other} 不支持转换"
                )))
            }
        }
        Ok(())
    }
}

pub(super) fn ensure_text_part_kind(
    kinds: &mut BTreeMap<u64, TextPartKind>,
    index: u64,
    incoming: TextPartKind,
    context: &str,
) -> Result<(), TransformError> {
    match kinds.get(&index) {
        Some(current) if *current != incoming => Err(TransformError(format!(
            "{context} content part 类型与已发送事件不一致"
        ))),
        Some(_) => Ok(()),
        None => {
            kinds.insert(index, incoming);
            Ok(())
        }
    }
}

pub(super) fn validate_tool_search_status(item: &Map<String, Value>) -> Result<(), TransformError> {
    match optional_string(item, "status", "Responses tool_search_call item")? {
        None => Ok(()),
        Some(status) if status == "in_progress" || status == "completed" => Ok(()),
        Some(status) => Err(TransformError(format!(
            "Responses tool_search_call item.status {status} 不支持"
        ))),
    }
}
