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
                let content = item
                    .get("content")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        TransformError(
                            "流式 response.output_item.added 的 message.content 必须是数组"
                                .to_string(),
                        )
                    })?;
                if !content.is_empty() {
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
                        item_id: Some(required_string(item, "id", "Responses message item")?),
                        text: String::new(),
                        parts: BTreeMap::new(),
                        part_kinds: BTreeMap::new(),
                        part_done: BTreeSet::new(),
                        closed: false,
                        item_done: false,
                    },
                );
            }
            "reasoning" => {
                allowed(
                    item,
                    &[
                        "type",
                        "id",
                        "status",
                        "summary",
                        "content",
                        "encrypted_content",
                    ],
                    "Responses reasoning item",
                )?;
                let item_id = required_string(item, "id", "Responses reasoning item")?;
                validate_reasoning_status(item, "Responses reasoning item")?;
                let summary = item.get("summary").ok_or_else(|| {
                    TransformError("Responses reasoning item 缺少 summary".to_string())
                })?;
                let summary_parts = parse_reasoning_summary(summary, "Responses reasoning item")?;
                let content_parts = item
                    .get("content")
                    .map(|value| parse_reasoning_content(value, "Responses reasoning item"))
                    .transpose()?
                    .unwrap_or_default();
                let encrypted_content = optional_nullable_string(
                    item,
                    "encrypted_content",
                    "Responses reasoning item",
                )?;
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
                        continuation: None,
                        incomplete: false,
                        emitted: false,
                        closed: false,
                        item_done: false,
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
                        item_id,
                        id,
                        name,
                        source_name,
                        namespace,
                        arguments,
                        closed: false,
                        item_done: false,
                    },
                );
            }
            "custom_tool_call" => {
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
                let namespace =
                    optional_string(item, "namespace", "Responses custom_tool_call item")?;
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
                append_event(
                    output,
                    "content_block_start",
                    json!({
                        "type": "content_block_start",
                        "index": content_index,
                        "content_block": { "type": "tool_use", "id": id, "name": name, "input": {} },
                    }),
                );
                let encoded_len = if input.is_empty() {
                    0
                } else {
                    let encoded = custom_arguments(&input)?;
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
            }
            "tool_search_call" => {
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
                let arguments = serde_json::to_string(&arguments).map_err(|_| {
                    TransformError("无法编码 Responses tool_search 参数".to_string())
                })?;
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

    pub(super) fn text_done(
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

    pub(super) fn tool_delta(
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

    pub(super) fn tool_done(
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
            if current != &arguments {
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

    pub(super) fn custom_tool_delta(
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
        let encoded = custom_arguments(input)?;
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

    pub(super) fn custom_tool_done(
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

    pub(super) fn refusal_delta(
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

    pub(super) fn refusal_done(
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
