//! Validate completed item snapshots and close their streamed state.

use super::*;

pub(super) fn finish_custom_item(
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
        closed,
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
    if (*item_done || *closed) && current != &final_input {
        return Err(TransformError(
            "Responses custom input changed after its block was closed".into(),
        ));
    }
    append_suffix(current, &final_input, "Responses 完整 custom 工具输入")?;
    let encoded = custom_arguments(current)?;
    let suffix = encoded
        .get(*encoded_len..)
        .ok_or_else(|| TransformError("Responses custom 工具输入顺序无效".to_string()))?
        .to_string();
    *encoded_len = encoded.len();
    emit_argument_suffix(output, *content_index, &suffix);
    *item_done = true;
    close_item(state, output);
    Ok(())
}

pub(super) fn finish_tool_search_item(
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
        emit_argument_suffix(output, *content_index, &suffix);
        *item_done = true;
    }
    if !*closed {
        close_item(state, output);
    }
    Ok(())
}

pub(super) fn finish_function_item(
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
    let final_arguments = required_value_string(
        item.get("arguments").ok_or_else(|| {
            TransformError("Responses function_call item 缺少 arguments".to_string())
        })?,
        "Responses function_call item.arguments",
    )?;
    let final_input = response_lifecycle::function_arguments(&final_arguments)?;
    if *item_done || *closed {
        if response_lifecycle::function_arguments(current)? != final_input {
            return Err(TransformError(
                "Responses 重复 function output item done 内容不一致".to_string(),
            ));
        }
    } else {
        let suffix = argument_suffix(current, &final_arguments)?;
        emit_argument_suffix(output, *content_index, &suffix);
        *item_done = true;
    }
    if !*closed {
        close_item(state, output);
    }
    Ok(())
}

pub(super) fn finish_message_item(
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
        part_done,
        item_done,
        closed,
        ..
    } = state
    else {
        unreachable!("message item state checked by caller")
    };
    validate_message_item(item, item_id)?;
    let (final_parts, final_kinds) = message_content_parts(item)?;
    if part_done
        .iter()
        .any(|index| current_parts.get(index) != final_parts.get(index))
    {
        return Err(TransformError(
            "Responses terminal text changed after content_part.done".into(),
        ));
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
    if *item_done || *closed {
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
            emit_text_suffix(output, *content_index, &suffix);
        }
        for (index, kind) in final_kinds {
            super::super::events::ensure_text_part_kind(
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

fn validate_message_item(
    item: &Map<String, Value>,
    item_id: &mut Option<String>,
) -> Result<(), TransformError> {
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
    Ok(())
}

fn message_content_parts(
    item: &Map<String, Value>,
) -> Result<(BTreeMap<u64, String>, BTreeMap<u64, TextPartKind>), TransformError> {
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
    Ok((final_parts, final_kinds))
}
