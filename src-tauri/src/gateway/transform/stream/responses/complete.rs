//! Completion, flushing, and final-part emission for the streamed conversion.

use super::*;

impl ResponsesToAnthropic {
    pub(super) fn content_part_done(
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
        super::events::ensure_text_part_kind(
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

    pub(super) fn item_done(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "response.output_item.done")?;
        let map = object(&value, "response.output_item.done")?;
        allowed(
            map,
            &["type", "sequence_number", "output_index", "item"],
            "response.output_item.done",
        )?;
        if required_string(map, "type", "response.output_item.done")? != "response.output_item.done"
        {
            return Err(TransformError(
                "response.output_item.done.type 无效".to_string(),
            ));
        }
        let output_index = required_index(map, "output_index")?;
        let item = object(
            map.get("item")
                .ok_or_else(|| TransformError("response.output_item.done 缺少 item".to_string()))?,
            "response.output_item.done.item",
        )?;
        let state = self.items.get_mut(&output_index).ok_or_else(|| {
            TransformError("Responses output item done 找不到起始 item".to_string())
        })?;
        match state {
            Item::CustomTool { .. } => finish_custom_item(state, item, output),
            Item::Tool { .. } => finish_function_item(state, item, output),
            Item::Text { .. } => finish_message_item(state, item, output),
            Item::ToolSearch { .. } => finish_tool_search_item(state, item, output),
            Item::Reasoning { .. } => {
                finish_reasoning_item(state, item, output, self.reasoning_transport.as_ref())
            }
        }
    }

    pub(super) fn complete(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "response.completed")?;
        let map = object(&value, "response.completed")?;
        allowed(
            map,
            &["type", "sequence_number", "response"],
            "response.completed",
        )?;
        if required_string(map, "type", "response.completed")? != "response.completed" {
            return Err(TransformError("response.completed.type 无效".to_string()));
        }
        let final_response = map
            .get("response")
            .ok_or_else(|| TransformError("response.completed 缺少 response".to_string()))?;
        let canonical =
            parse_responses_complete(final_response, self.reasoning_transport.as_ref())?;
        merge(&mut self.id, &canonical.id, "Responses SSE id")?;
        merge(&mut self.model, &canonical.model, "Responses SSE model")?;
        self.start(output)?;
        self.flush_final(&canonical.content, output)?;
        append_event(
            output,
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": { "stop_reason": anthropic_stop_reason(canonical.stop), "stop_sequence": null },
                "usage": { "output_tokens": canonical.usage.output_tokens.unwrap_or(0) },
            }),
        );
        append_event(output, "message_stop", json!({ "type": "message_stop" }));
        self.completed = true;
        Ok(())
    }

    pub(super) fn flush_final(
        &mut self,
        expected: &[ResponsePart],
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let existing_out_of_range = self
            .items
            .keys()
            .any(|index| *index as usize >= expected.len());
        if existing_out_of_range {
            return Err(TransformError(
                "Responses 完整响应少于已发送的流式输出项".to_string(),
            ));
        }
        for (index, part) in expected.iter().enumerate() {
            let output_index = index as u64;
            if self.items.contains_key(&output_index) {
                if let Some(pending) = self.pending_indexed.remove(&output_index) {
                    output.extend(pending);
                }
                self.finish_existing(output_index, part, output)?;
                self.sync_closed(output_index);
            } else {
                let content_index = self.next_content_index;
                self.next_content_index += 1;
                emit_complete_part(output, content_index, part)?;
                self.closed_indices.insert(output_index);
            }
            self.flush_ready(output);
        }
        if !self.pending_indexed.is_empty() {
            return Err(TransformError(
                "Responses 完整响应与流式输出项顺序不一致".to_string(),
            ));
        }
        Ok(())
    }

    pub(super) fn finish_existing(
        &mut self,
        output_index: u64,
        expected: &ResponsePart,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        match (self.items.get_mut(&output_index), expected) {
            (
                Some(Item::Text {
                    content_index,
                    text,
                    closed,
                    item_done,
                    ..
                }),
                ResponsePart::Text(final_text),
            ) => {
                let suffix = append_suffix(text, final_text, "Responses 完整文本")?;
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
                if !*closed {
                    append_event(
                        output,
                        "content_block_stop",
                        json!({ "type": "content_block_stop", "index": *content_index }),
                    );
                    *closed = true;
                }
                *item_done = true;
                Ok(())
            }
            (
                Some(Item::Tool {
                    content_index,
                    id,
                    name,
                    source_name,
                    namespace,
                    arguments,
                    closed,
                    item_done,
                    ..
                }),
                ResponsePart::ToolCall {
                    id: expected_id,
                    name: expected_name,
                    namespace: expected_namespace,
                    kind: expected_kind,
                    input,
                },
            ) => {
                if *expected_kind != ToolKind::Function {
                    return Err(TransformError(
                        "Responses 完整输出类型与流式 function 工具不一致".to_string(),
                    ));
                }
                let expected_name = render_target_name(
                    UpstreamProtocol::AnthropicMessages,
                    expected_namespace.as_deref(),
                    expected_name,
                    *expected_kind,
                )?;
                if id != expected_id
                    || name != &expected_name
                    || source_name.as_str() != expected_name.as_str()
                    || namespace != expected_namespace
                {
                    return Err(TransformError(
                        "Responses 完整工具调用与流式输出不一致".to_string(),
                    ));
                }
                let final_arguments = serde_json::to_string(input)
                    .map_err(|_| TransformError("无法编码 Responses 工具参数".to_string()))?;
                let suffix = argument_suffix(arguments, &final_arguments)?;
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
                *item_done = true;
                Ok(())
            }
            (
                Some(Item::CustomTool {
                    content_index,
                    id,
                    name,
                    namespace,
                    input: current,
                    encoded_len,
                    closed,
                    item_done,
                    ..
                }),
                ResponsePart::ToolCall {
                    id: expected_id,
                    name: expected_name,
                    namespace: expected_namespace,
                    kind: expected_kind,
                    input,
                },
            ) if *expected_kind == ToolKind::Custom => {
                if id != expected_id || name != expected_name || namespace != expected_namespace {
                    return Err(TransformError(
                        "Responses 完整 custom 工具调用与流式输出不一致".to_string(),
                    ));
                }
                let final_input = custom_input(input)?;
                append_suffix(current, &final_input, "Responses 完整 custom 工具输入")?;
                let encoded = custom_arguments(current)?;
                let suffix = encoded
                    .get(*encoded_len..)
                    .ok_or_else(|| TransformError("Responses custom 工具输入顺序无效".to_string()))?
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
                *item_done = true;
                Ok(())
            }
            (
                Some(Item::ToolSearch {
                    content_index,
                    id,
                    arguments: current,
                    closed,
                    item_done,
                    ..
                }),
                ResponsePart::ToolCall {
                    id: expected_id,
                    name: expected_name,
                    namespace,
                    kind: expected_kind,
                    input,
                },
            ) if *expected_kind == ToolKind::ToolSearch => {
                if id != expected_id
                    || expected_name != crate::gateway::transform::CODEX_TOOL_SEARCH_NAME
                    || namespace.is_some()
                    || !input.is_object()
                {
                    return Err(TransformError(
                        "Responses 完整 tool_search 调用与流式输出不一致".to_string(),
                    ));
                }
                let final_arguments = serde_json::to_string(input).map_err(|_| {
                    TransformError("无法编码 Responses tool_search 参数".to_string())
                })?;
                let suffix = argument_suffix(current, &final_arguments)?;
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
                *item_done = true;
                if !*closed {
                    append_event(
                        output,
                        "content_block_stop",
                        json!({ "type": "content_block_stop", "index": *content_index }),
                    );
                    *closed = true;
                }
                Ok(())
            }
            (Some(state @ Item::Reasoning { .. }), ResponsePart::Reasoning(expected)) => {
                finish_reasoning_expected(state, expected, output)
            }
            _ => Err(TransformError(
                "Responses 完整输出类型与流式输出不一致".to_string(),
            )),
        }
    }

    pub(super) fn start(&mut self, output: &mut Vec<u8>) -> Result<(), TransformError> {
        if self.started {
            return Ok(());
        }
        let id = self
            .id
            .as_deref()
            .ok_or_else(|| TransformError("Responses SSE 缺少 id".to_string()))?;
        let model = self
            .model
            .as_deref()
            .ok_or_else(|| TransformError("Responses SSE 缺少 model".to_string()))?;
        append_event(
            output,
            "message_start",
            json!({
                "type": "message_start",
                "message": {
                    "id": id,
                    "type": "message",
                    "role": "assistant",
                    "model": model,
                    "content": [],
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": { "input_tokens": 0, "output_tokens": 0 },
                },
            }),
        );
        self.started = true;
        Ok(())
    }

    pub(super) fn require_started(&self) -> Result<(), TransformError> {
        if self.started {
            Ok(())
        } else {
            Err(TransformError(
                "Responses SSE 在 response.created 前发送内容".to_string(),
            ))
        }
    }
}

fn finish_custom_item(
    state: &mut Item,
    item: &Map<String, Value>,
    output: &mut Vec<u8>,
) -> Result<(), TransformError> {
    let Item::CustomTool {
        content_index,
        item_id,
        id,
        name,
        namespace,
        input: current,
        encoded_len,
        item_done,
        ..
    } = state
    else {
        unreachable!("custom item state checked by caller")
    };
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
    if required_string(item, "type", "Responses custom_tool_call item")? != "custom_tool_call" {
        return Err(TransformError(
            "Responses output item done 类型与 custom 工具不一致".to_string(),
        ));
    }
    if let Some(status) = optional_string(item, "status", "Responses custom_tool_call item")? {
        if status != "completed" {
            return Err(TransformError(
                "Responses custom_tool_call item.status 必须是 completed".to_string(),
            ));
        }
    }
    merge_done_item_id(
        item_id,
        Some(required_string(
            item,
            "id",
            "Responses custom_tool_call item",
        )?),
    )?;
    if required_string(item, "call_id", "Responses custom_tool_call item")? != *id
        || required_string(item, "name", "Responses custom_tool_call item")? != *name
        || optional_string(item, "namespace", "Responses custom_tool_call item")? != *namespace
    {
        return Err(TransformError(
            "Responses 完整 custom 工具调用与流式输出不一致".to_string(),
        ));
    }
    let final_input = required_value_string(
        item.get("input").ok_or_else(|| {
            TransformError("Responses custom_tool_call item 缺少 input".to_string())
        })?,
        "Responses custom_tool_call item.input",
    )?;
    append_suffix(current, &final_input, "Responses 完整 custom 工具输入")?;
    let encoded = custom_arguments(current)?;
    let suffix = encoded
        .get(*encoded_len..)
        .ok_or_else(|| TransformError("Responses custom 工具输入顺序无效".to_string()))?
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
    *item_done = true;
    close_item(state, output);
    Ok(())
}

fn finish_tool_search_item(
    state: &mut Item,
    item: &Map<String, Value>,
    output: &mut Vec<u8>,
) -> Result<(), TransformError> {
    let Item::ToolSearch {
        content_index,
        item_id,
        id,
        arguments: current,
        closed,
        item_done,
    } = state
    else {
        unreachable!("tool search item state checked by caller")
    };
    allowed(
        item,
        &["type", "id", "call_id", "status", "execution", "arguments"],
        "Responses tool_search_call item",
    )?;
    if required_string(item, "type", "Responses tool_search_call item")? != "tool_search_call" {
        return Err(TransformError(
            "Responses output item done 类型与 tool_search 不一致".to_string(),
        ));
    }
    if optional_string(item, "execution", "Responses tool_search_call item")?.as_deref()
        != Some("client")
    {
        return Err(TransformError(
            "Responses tool_search_call.execution 必须是 client".to_string(),
        ));
    }
    super::events::validate_tool_search_status(item)?;
    if optional_string(item, "status", "Responses tool_search_call item")?.as_deref()
        != Some("completed")
    {
        return Err(TransformError(
            "Responses tool_search_call item.status 必须是 completed".to_string(),
        ));
    }
    merge_done_item_id(
        item_id,
        Some(required_string(
            item,
            "id",
            "Responses tool_search_call item",
        )?),
    )?;
    if required_string(item, "call_id", "Responses tool_search_call item")? != *id {
        return Err(TransformError(
            "Responses 完整 tool_search 调用与流式输出不一致".to_string(),
        ));
    }
    let arguments = item.get("arguments").cloned().unwrap_or_else(|| json!({}));
    if !arguments.is_object() {
        return Err(TransformError(
            "Responses tool_search_call.arguments 必须是对象".to_string(),
        ));
    }
    let final_arguments = serde_json::to_string(&arguments)
        .map_err(|_| TransformError("无法编码 Responses tool_search 参数".to_string()))?;
    if *item_done {
        if *current != final_arguments {
            return Err(TransformError(
                "Responses 重复 tool_search output item done 内容不一致".to_string(),
            ));
        }
    } else {
        let suffix = argument_suffix(current, &final_arguments)?;
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
        *item_done = true;
    }
    if !*closed {
        close_item(state, output);
    }
    Ok(())
}

pub(super) fn close_item(item: &mut Item, output: &mut Vec<u8>) {
    let (content_index, closed) = match item {
        Item::Text {
            content_index,
            closed,
            ..
        }
        | Item::Tool {
            content_index,
            closed,
            ..
        }
        | Item::CustomTool {
            content_index,
            closed,
            ..
        }
        | Item::ToolSearch {
            content_index,
            closed,
            ..
        }
        | Item::Reasoning {
            content_index,
            closed,
            ..
        } => (*content_index, closed),
    };
    if !*closed {
        append_event(
            output,
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": content_index }),
        );
        *closed = true;
    }
}

fn merge_done_item_id(
    expected: &mut Option<String>,
    incoming: Option<String>,
) -> Result<(), TransformError> {
    match (expected.as_ref(), incoming) {
        (Some(expected), Some(incoming)) if expected != &incoming => Err(TransformError(
            "Responses output item done.id 与起始 item 不一致".to_string(),
        )),
        (Some(_), None) => Err(TransformError(
            "Responses output item done 缺少起始 item 的 id".to_string(),
        )),
        (None, Some(incoming)) => {
            *expected = Some(incoming);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn finish_function_item(
    state: &mut Item,
    item: &Map<String, Value>,
    output: &mut Vec<u8>,
) -> Result<(), TransformError> {
    let Item::Tool {
        content_index,
        item_done,
        item_id,
        id,
        source_name,
        namespace,
        arguments: current,
        closed,
        ..
    } = state
    else {
        unreachable!("function item state checked by caller")
    };
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
    if required_string(item, "type", "Responses function_call item")? != "function_call"
        || required_string(item, "call_id", "Responses function_call item")? != *id
        || required_string(item, "name", "Responses function_call item")? != *source_name
        || optional_string(item, "namespace", "Responses function_call item")? != *namespace
    {
        return Err(TransformError(
            "Responses 完整 function 工具调用与流式输出不一致".to_string(),
        ));
    }
    merge_done_item_id(
        item_id,
        Some(required_string(item, "id", "Responses function_call item")?),
    )?;
    if let Some(status) = optional_string(item, "status", "Responses function_call item")? {
        if status != "completed" {
            return Err(TransformError(
                "Responses function_call item.status 必须是 completed".to_string(),
            ));
        }
    }
    let final_arguments = required_value_string(
        item.get("arguments").ok_or_else(|| {
            TransformError("Responses function_call item 缺少 arguments".to_string())
        })?,
        "Responses function_call item.arguments",
    )?;
    if *item_done {
        if *current != final_arguments {
            return Err(TransformError(
                "Responses 重复 function output item done 内容不一致".to_string(),
            ));
        }
    } else {
        let suffix = argument_suffix(current, &final_arguments)?;
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
        *item_done = true;
    }
    if !*closed {
        close_item(state, output);
    }
    Ok(())
}

fn finish_message_item(
    state: &mut Item,
    item: &Map<String, Value>,
    output: &mut Vec<u8>,
) -> Result<(), TransformError> {
    let Item::Text {
        content_index,
        item_id,
        text: current,
        parts: current_parts,
        part_kinds,
        item_done,
        closed,
        ..
    } = state
    else {
        unreachable!("message item state checked by caller")
    };
    allowed(
        item,
        &["type", "id", "status", "role", "content"],
        "Responses message item",
    )?;
    if required_string(item, "type", "Responses message item")? != "message"
        || required_string(item, "role", "Responses message item")? != "assistant"
    {
        return Err(TransformError(
            "Responses 完整 message item 与流式输出不一致".to_string(),
        ));
    }
    merge_done_item_id(
        item_id,
        Some(required_string(item, "id", "Responses message item")?),
    )?;
    if let Some(status) = optional_string(item, "status", "Responses message item")? {
        if status != "completed" {
            return Err(TransformError(
                "Responses message item.status 必须是 completed".to_string(),
            ));
        }
    }
    let parts = item
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| TransformError("Responses message item.content 必须是数组".to_string()))?;
    let mut final_parts = BTreeMap::new();
    let mut final_kinds = BTreeMap::new();
    for (index, part) in parts.iter().enumerate() {
        let part = object(part, "Responses message item.content")?;
        allowed(
            part,
            &["type", "text", "refusal", "annotations"],
            "Responses message item.content",
        )?;
        let (value, kind) =
            match required_string(part, "type", "Responses message item.content")?.as_str() {
                "output_text" => (
                    string_value(part, "text", "Responses message item.content")?,
                    TextPartKind::Output,
                ),
                "refusal" => (
                    string_value(part, "refusal", "Responses message item.content")?,
                    TextPartKind::Refusal,
                ),
                other => {
                    return Err(TransformError(format!(
                        "Responses message item.content.type {other} 不支持转换"
                    )))
                }
            };
        final_parts.insert(index as u64, value);
        final_kinds.insert(index as u64, kind);
    }
    if current_parts
        .keys()
        .any(|index| !final_parts.contains_key(index))
        || part_kinds
            .keys()
            .any(|index| !final_kinds.contains_key(index))
    {
        return Err(TransformError(
            "Responses 完整 message content part 与流式输出不一致".to_string(),
        ));
    }
    if *item_done {
        if current_parts != &final_parts || part_kinds != &final_kinds {
            return Err(TransformError(
                "Responses 重复 message output item done 内容不一致".to_string(),
            ));
        }
    } else {
        for (index, value) in &final_parts {
            let suffix = replace_text_part(
                current_parts,
                current,
                *index,
                value,
                "Responses 完整 message 文本",
            )?;
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
        }
        for (index, kind) in final_kinds {
            super::events::ensure_text_part_kind(
                part_kinds,
                index,
                kind,
                "Responses message item",
            )?;
        }
        *item_done = true;
    }
    if !*closed {
        close_item(state, output);
    }
    Ok(())
}

pub(super) fn emit_complete_part(
    output: &mut Vec<u8>,
    content_index: u64,
    part: &ResponsePart,
) -> Result<(), TransformError> {
    match part {
        ResponsePart::Text(text) => {
            append_event(
                output,
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": content_index,
                    "content_block": { "type": "text", "text": "" },
                }),
            );
            if !text.is_empty() {
                append_event(
                    output,
                    "content_block_delta",
                    json!({
                        "type": "content_block_delta",
                        "index": content_index,
                        "delta": { "type": "text_delta", "text": text },
                    }),
                );
            }
        }
        ResponsePart::Reasoning(reasoning) => {
            append_event(
                output,
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": content_index,
                    "content_block": {
                        "type": "redacted_thinking",
                        "data": reasoning.continuation,
                    },
                }),
            );
        }
        ResponsePart::ToolCall {
            id,
            name,
            namespace,
            kind,
            input,
        } => {
            let arguments = if *kind == ToolKind::Custom {
                custom_arguments(&custom_input(input)?)?
            } else {
                serde_json::to_string(input)
                    .map_err(|_| TransformError("无法编码 Responses 工具参数".to_string()))?
            };
            append_event(
                output,
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": content_index,
                    "content_block": {
                        "type": "tool_use",
                        "id": id,
                        "name": render_target_name(
                            UpstreamProtocol::AnthropicMessages,
                            namespace.as_deref(),
                            name,
                            *kind,
                        )?,
                        "input": {},
                    },
                }),
            );
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
    }
    append_event(
        output,
        "content_block_stop",
        json!({ "type": "content_block_stop", "index": content_index }),
    );
    Ok(())
}

fn custom_input(input: &Value) -> Result<String, TransformError> {
    match input {
        Value::String(value) => Ok(value.clone()),
        Value::Object(map) if map.len() == 1 => map
            .get("input")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| TransformError("custom 工具输入必须是字符串".to_string())),
        _ => Err(TransformError("custom 工具输入必须是字符串".to_string())),
    }
}

pub(super) fn append_suffix(
    current: &mut String,
    final_value: &str,
    context: &str,
) -> Result<String, TransformError> {
    let Some(suffix) = final_value.strip_prefix(current.as_str()) else {
        return Err(TransformError(format!("{context} 与已发送流式内容不一致")));
    };
    let suffix = suffix.to_string();
    current.push_str(&suffix);
    Ok(suffix)
}

pub(super) fn append_text_part(
    parts: &mut BTreeMap<u64, String>,
    combined: &mut String,
    content_index: u64,
    delta: &str,
    context: &str,
) -> Result<String, TransformError> {
    parts.entry(content_index).or_default().push_str(delta);
    recompute_text_parts(parts, combined, context)
}

pub(super) fn merge_text_part_snapshot(
    parts: &mut BTreeMap<u64, String>,
    combined: &mut String,
    content_index: u64,
    snapshot: &str,
    context: &str,
) -> Result<String, TransformError> {
    let current = parts.entry(content_index).or_default();
    if current.is_empty() {
        current.push_str(snapshot);
    } else if let Some(suffix) = snapshot.strip_prefix(current.as_str()) {
        current.push_str(suffix);
    } else if !current.starts_with(snapshot) {
        return Err(TransformError(format!("{context} 与已发送流式内容不一致")));
    }
    recompute_text_parts(parts, combined, context)
}

pub(super) fn replace_text_part(
    parts: &mut BTreeMap<u64, String>,
    combined: &mut String,
    content_index: u64,
    final_text: &str,
    context: &str,
) -> Result<String, TransformError> {
    let current = parts.entry(content_index).or_default();
    append_suffix(current, final_text, context)?;
    recompute_text_parts(parts, combined, context)
}

fn recompute_text_parts(
    parts: &BTreeMap<u64, String>,
    combined: &mut String,
    context: &str,
) -> Result<String, TransformError> {
    let mut joined = String::new();
    for value in parts.values() {
        joined.push_str(value);
    }
    let suffix = joined
        .strip_prefix(combined.as_str())
        .ok_or_else(|| TransformError(format!("{context} 与已发送流式内容不一致")))?
        .to_string();
    combined.push_str(&suffix);
    Ok(suffix)
}

pub(super) fn argument_suffix(
    current: &mut String,
    final_value: &str,
) -> Result<String, TransformError> {
    if let Some(suffix) = final_value.strip_prefix(current.as_str()) {
        let suffix = suffix.to_string();
        current.push_str(&suffix);
        return Ok(suffix);
    }
    let current_value: Value = serde_json::from_str(current)
        .map_err(|_| TransformError("Responses 流式工具参数与完整参数不一致".to_string()))?;
    let final_value_json: Value = serde_json::from_str(final_value)
        .map_err(|_| TransformError("Responses 完整工具参数不是 JSON".to_string()))?;
    if current_value == final_value_json {
        Ok(String::new())
    } else {
        Err(TransformError(
            "Responses 流式工具参数与完整参数不一致".to_string(),
        ))
    }
}

pub(super) fn merge(
    destination: &mut Option<String>,
    incoming: &str,
    context: &str,
) -> Result<(), TransformError> {
    match destination {
        Some(current) if current != incoming => {
            Err(TransformError(format!("{context} 在流中变化")))
        }
        Some(_) => Ok(()),
        None => {
            *destination = Some(incoming.to_string());
            Ok(())
        }
    }
}
