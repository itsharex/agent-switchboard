//! Item creation from output_item.added, including initial payloads.

use super::*;

impl ResponsesToAnthropic {
    pub(super) fn add_message_item(
        &mut self,
        output_index: u64,
        content_index: u64,
        item: &Map<String, Value>,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
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
        let content = item
            .get("content")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                TransformError(
                    "流式 response.output_item.added 的 message.content 必须是数组".to_string(),
                )
            })?;
        if !content.is_empty() {
            return Err(TransformError(
                "流式 response.output_item.added 的 message.content 必须为空数组".to_string(),
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
                item_id: Some(required_string(item, "id", "Responses message item")?),
                text: String::new(),
                parts: BTreeMap::new(),
                part_kinds: BTreeMap::new(),
                part_done: BTreeSet::new(),
                closed: false,
                item_done: false,
            },
        );
        Ok(())
    }

    pub(super) fn add_reasoning_item(
        &mut self,
        output_index: u64,
        content_index: u64,
        item: &Map<String, Value>,
    ) -> Result<(), TransformError> {
        let item_id = required_string(item, "id", "Responses reasoning item")?;
        validate_reasoning_status(item, "Responses reasoning item")?;
        let summary_parts = item
            .get("summary")
            .map(|value| parse_reasoning_summary(value, "Responses reasoning item"))
            .transpose()?
            .unwrap_or_default();
        let content_parts = item
            .get("content")
            .map(|value| parse_reasoning_content(value, "Responses reasoning item"))
            .transpose()?
            .unwrap_or_default();
        let encrypted_content =
            optional_nullable_string(item, "encrypted_content", "Responses reasoning item")?;
        self.items.insert(
            output_index,
            Item::Reasoning {
                content_index,
                item_id: Some(item_id),
                text: reasoning_text(&summary_parts, &content_parts),
                summary_parts,
                content_parts,
                summary_done: BTreeSet::new(),
                content_done: BTreeSet::new(),
                encrypted_content,
                native_item: None,
                continuation: None,
                incomplete: false,
                emitted: false,
                closed: false,
                item_done: false,
            },
        );
        Ok(())
    }

    pub(super) fn add_function_item(
        &mut self,
        output_index: u64,
        content_index: u64,
        item: &Map<String, Value>,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
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
        let item_id = Some(required_string(item, "id", "Responses function_call item")?);
        let namespace = optional_string(item, "namespace", "Responses function_call item")?;
        let source_name = required_string(item, "name", "Responses function_call item")?;
        let name = render_target_name(
            UpstreamProtocol::AnthropicMessages,
            namespace.as_deref(),
            &source_name,
            crate::gateway::transform::ToolKind::Function,
        )?;
        let arguments = item
            .get("arguments")
            .map(|value| required_value_string(value, "Responses function_call arguments"))
            .transpose()?
            .unwrap_or_default();
        start_tool_block(output, content_index, &id, &name);
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
                item_id,
                id,
                source_name,
                namespace,
                arguments,
                closed: false,
                item_done: false,
            },
        );
        Ok(())
    }

    pub(super) fn add_custom_item(
        &mut self,
        output_index: u64,
        content_index: u64,
        item: &Map<String, Value>,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        allowed(
            item,
            &[
                "type",
                "id",
                "call_id",
                "name",
                "namespace",
                "input",
                "status",
            ],
            "Responses custom_tool_call item",
        )?;
        let id = required_string(item, "call_id", "Responses custom_tool_call item")?;
        let item_id = Some(required_string(
            item,
            "id",
            "Responses custom_tool_call item",
        )?);
        let namespace = optional_string(item, "namespace", "Responses custom_tool_call item")?;
        let source_name = required_string(item, "name", "Responses custom_tool_call item")?;
        let name = render_target_name(
            UpstreamProtocol::AnthropicMessages,
            namespace.as_deref(),
            &source_name,
            ToolKind::Custom,
        )?;
        let input = item
            .get("input")
            .map(|value| required_value_string(value, "Responses custom_tool_call input"))
            .transpose()?
            .unwrap_or_default();
        start_tool_block(output, content_index, &id, &name);
        let encoded_len = if input.is_empty() {
            0
        } else {
            let encoded = custom_argument_prefix(&input)?;
            append_event(
                output,
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": content_index,
                    "delta": { "type": "input_json_delta", "partial_json": encoded },
                }),
            );
            encoded.len()
        };
        self.items.insert(
            output_index,
            Item::CustomTool {
                content_index,
                item_id,
                id,
                name: source_name,
                namespace,
                input,
                encoded_len,
                closed: false,
                item_done: false,
            },
        );
        Ok(())
    }

    pub(super) fn add_tool_search_item(
        &mut self,
        output_index: u64,
        content_index: u64,
        item: &Map<String, Value>,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        allowed(
            item,
            &["type", "id", "call_id", "status", "execution", "arguments"],
            "Responses tool_search_call item",
        )?;
        let id = required_string(item, "call_id", "Responses tool_search_call item")?;
        if optional_string(item, "execution", "Responses tool_search_call item")?.as_deref()
            != Some("client")
        {
            return Err(TransformError(
                "Responses tool_search_call.execution 必须是 client".to_string(),
            ));
        }
        validate_tool_search_status(item)?;
        let arguments = item.get("arguments").cloned().unwrap_or_else(|| json!({}));
        if !arguments.is_object() {
            return Err(TransformError(
                "Responses tool_search_call.arguments 必须是对象".to_string(),
            ));
        }
        let arguments = serde_json::to_string(&arguments)
            .map_err(|_| TransformError("无法编码 Responses tool_search 参数".to_string()))?;
        append_event(
            output,
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": content_index,
                "content_block": {
                    "type": "tool_use",
                    "id": id,
                    "name": crate::gateway::transform::CODEX_TOOL_SEARCH_NAME,
                    "input": {}
                },
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
            Item::ToolSearch {
                content_index,
                item_id: Some(required_string(
                    item,
                    "id",
                    "Responses tool_search_call item",
                )?),
                id,
                arguments,
                closed: false,
                item_done: false,
            },
        );
        Ok(())
    }
}

fn start_tool_block(output: &mut Vec<u8>, content_index: u64, id: &str, name: &str) {
    append_event(
        output,
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": content_index,
            "content_block": { "type": "tool_use", "id": id, "name": name, "input": {} },
        }),
    );
}
