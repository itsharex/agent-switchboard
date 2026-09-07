//! Streaming event handlers that assemble incremental items.

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
        let content_index = self.next_content_index;
        self.next_content_index += 1;
        match required_string(item, "type", "response output item")?.as_str() {
            "message" => {
                allowed(
                    item,
                    &["type", "id", "status", "role", "content"],
                    "Responses message item",
                )?;
                if required_string(item, "role", "Responses message item")? != "assistant" {
                    return Err(TransformError(
                        "Responses message item 不是 assistant".to_string(),
                    ));
                }
                if !item
                    .get("content")
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty)
                {
                    return Err(TransformError(
                        "流式 response.output_item.added 的 message.content 必须为空数组"
                            .to_string(),
                    ));
                }
                append_event(
                    output,
                    "content_block_start",
                    json!({
                        "type": "content_block_start",
                        "index": content_index,
                        "content_block": { "type": "text", "text": "" },
                    }),
                );
                self.items.insert(
                    output_index,
                    Item::Text {
                        content_index,
                        text: String::new(),
                    },
                );
            }
            "function_call" => {
                allowed(
                    item,
                    &[
                        "type",
                        "id",
                        "call_id",
                        "name",
                        "namespace",
                        "arguments",
                        "status",
                    ],
                    "Responses function_call item",
                )?;
                let id = required_string(item, "call_id", "Responses function_call item")?;
                let source_name = required_string(item, "name", "Responses function_call item")?;
                let name = render_target_name(
                    UpstreamProtocol::AnthropicMessages,
                    optional_string(item, "namespace", "Responses function_call item")?.as_deref(),
                    &source_name,
                )?;
                let arguments = item
                    .get("arguments")
                    .map(|value| required_value_string(value, "Responses function_call arguments"))
                    .transpose()?
                    .unwrap_or_default();
                append_event(
                    output,
                    "content_block_start",
                    json!({
                        "type": "content_block_start",
                        "index": content_index,
                        "content_block": { "type": "tool_use", "id": id, "name": name, "input": {} },
                    }),
                );
                if !arguments.is_empty() {
                    append_event(
                        output,
                        "content_block_delta",
                        json!({
                            "type": "content_block_delta",
                            "index": content_index,
                            "delta": { "type": "input_json_delta", "partial_json": arguments },
                        }),
                    );
                }
                self.items.insert(
                    output_index,
                    Item::Tool {
                        content_index,
                        id,
                        name,
                        arguments,
                    },
                );
            }
            other => {
                return Err(TransformError(format!(
                    "Responses output item {other} 不支持转换"
                )))
            }
        }
        Ok(())
    }

    pub(super) fn content_part_added(
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
        if required_index(map, "content_index")? != 0 {
            return Err(TransformError(
                "仅支持 Responses content_index=0".to_string(),
            ));
        }
        let output_index = required_index(map, "output_index")?;
        let part = object(
            map.get("part").ok_or_else(|| {
                TransformError("response.content_part.added 缺少 part".to_string())
            })?,
            "response.content_part.added.part",
        )?;
        allowed(
            part,
            &["type", "text", "annotations"],
            "Responses output text part",
        )?;
        if required_string(part, "type", "Responses output text part")? != "output_text" {
            return Err(TransformError(
                "Responses content part 不是 output_text".to_string(),
            ));
        }
        let initial = required_string(part, "text", "Responses output text part")?;
        let (content_index, suffix) = match self.items.get_mut(&output_index) {
            Some(Item::Text {
                content_index,
                text,
            }) => {
                let suffix = append_suffix(text, &initial, "Responses content part")?;
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

    pub(super) fn text_delta(
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
                "delta",
            ],
            "response.output_text.delta",
        )?;
        if required_string(map, "type", "response.output_text.delta")?
            != "response.output_text.delta"
            || required_index(map, "content_index")? != 0
        {
            return Err(TransformError(
                "response.output_text.delta 字段无效".to_string(),
            ));
        }
        let output_index = required_index(map, "output_index")?;
        let delta = required_string(map, "delta", "response.output_text.delta")?;
        let content_index = match self.items.get_mut(&output_index) {
            Some(Item::Text {
                content_index,
                text,
            }) => {
                text.push_str(&delta);
                *content_index
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
        if !delta.is_empty() {
            append_event(
                output,
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": content_index,
                    "delta": { "type": "text_delta", "text": delta },
                }),
            );
        }
        Ok(())
    }

    pub(super) fn tool_delta(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "response.function_call_arguments.delta")?;
        let map = object(&value, "response.function_call_arguments.delta")?;
        allowed(
            map,
            &["type", "sequence_number", "output_index", "delta"],
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
        let delta = required_string(map, "delta", "response.function_call_arguments.delta")?;
        let content_index = match self.items.get_mut(&output_index) {
            Some(Item::Tool {
                content_index,
                arguments,
                ..
            }) => {
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
}
