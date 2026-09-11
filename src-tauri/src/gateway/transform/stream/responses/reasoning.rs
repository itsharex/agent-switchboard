//! Reasoning item handling for the Responses to Anthropic stream bridge.

use super::*;
use crate::gateway::transform::Reasoning;

pub(super) fn validate_reasoning_status(
    item: &Map<String, Value>,
    context: &str,
) -> Result<(), TransformError> {
    match item.get("status") {
        None => Ok(()),
        Some(Value::String(status)) if matches!(status.as_str(), "in_progress" | "completed") => {
            Ok(())
        }
        Some(Value::String(status)) if status == "incomplete" => Err(TransformError(format!(
            "{context}.status 不支持 incomplete reasoning"
        ))),
        Some(Value::String(_)) => Err(TransformError(format!(
            "{context}.status 必须是 in_progress 或 completed"
        ))),
        Some(_) => Err(TransformError(format!("{context}.status 必须是字符串"))),
    }
}

pub(super) fn parse_reasoning_summary(
    value: &Value,
    context: &str,
) -> Result<BTreeMap<u64, String>, TransformError> {
    let values = value
        .as_array()
        .ok_or_else(|| TransformError(format!("{context}.summary 必须是数组")))?;
    let mut parts = BTreeMap::new();
    for (index, value) in values.iter().enumerate() {
        let part = object(value, &format!("{context}.summary[{index}]"))?;
        allowed(part, &["type", "text"], "Responses reasoning summary part")?;
        if required_string(part, "type", "Responses reasoning summary part")? != "summary_text" {
            return Err(TransformError(
                "Responses reasoning summary part.type 必须是 summary_text".to_string(),
            ));
        }
        let text = string_value(part, "text", "Responses reasoning summary part")?;
        parts.insert(index as u64, text);
    }
    Ok(parts)
}

pub(super) fn parse_reasoning_content(
    value: &Value,
    context: &str,
) -> Result<BTreeMap<u64, String>, TransformError> {
    let values = value
        .as_array()
        .ok_or_else(|| TransformError(format!("{context}.content 必须是数组")))?;
    let mut parts = BTreeMap::new();
    for (index, value) in values.iter().enumerate() {
        let part = object(value, &format!("{context}.content[{index}]"))?;
        allowed(part, &["type", "text"], "Responses reasoning content part")?;
        if required_string(part, "type", "Responses reasoning content part")? != "reasoning_text" {
            return Err(TransformError(
                "Responses reasoning content part.type 必须是 reasoning_text".to_string(),
            ));
        }
        let text = string_value(part, "text", "Responses reasoning content part")?;
        parts.insert(index as u64, text);
    }
    Ok(parts)
}

pub(super) fn reasoning_text(
    summary_parts: &BTreeMap<u64, String>,
    content_parts: &BTreeMap<u64, String>,
) -> String {
    summary_parts
        .values()
        .chain(content_parts.values())
        .fold(String::new(), |mut text, part| {
            text.push_str(part);
            text
        })
}

impl ResponsesToAnthropic {
    pub(super) fn reasoning_part_added(&mut self, frame: &Frame) -> Result<(), TransformError> {
        let event = "response.reasoning_summary_part.added";
        let value = json_data(frame, event)?;
        let map = object(&value, event)?;
        allowed(
            map,
            &[
                "type",
                "item_id",
                "output_index",
                "summary_index",
                "part",
                "sequence_number",
            ],
            event,
        )?;
        if required_string(map, "type", event)? != event {
            return Err(TransformError(format!("{event}.type 无效")));
        }
        let output_index = required_index(map, "output_index")?;
        let summary_index = required_index(map, "summary_index")?;
        let item_id = required_string(map, "item_id", event)?;
        let part = reasoning_summary_part(map, event)?;
        let state = self.reasoning_state(output_index, event)?;
        let Item::Reasoning {
            item_id: expected_id,
            summary_parts,
            content_parts,
            summary_done,
            text,
            closed,
            ..
        } = state
        else {
            return Err(TransformError(
                "Responses reasoning summary part 指向非 reasoning item".to_string(),
            ));
        };
        merge_reasoning_id(expected_id, &item_id, event)?;
        if *closed || summary_done.contains(&summary_index) {
            return Err(TransformError(format!(
                "{event} 出现在 reasoning part 完成后"
            )));
        }
        merge_reasoning_snapshot(summary_parts, summary_index, &part, event)?;
        *text = reasoning_text(summary_parts, content_parts);
        Ok(())
    }

    pub(super) fn reasoning_part_done(&mut self, frame: &Frame) -> Result<(), TransformError> {
        let event = "response.reasoning_summary_part.done";
        let value = json_data(frame, event)?;
        let map = object(&value, event)?;
        allowed(
            map,
            &[
                "type",
                "item_id",
                "output_index",
                "summary_index",
                "status",
                "part",
                "sequence_number",
            ],
            event,
        )?;
        if required_string(map, "type", event)? != event {
            return Err(TransformError(format!("{event}.type 无效")));
        }
        let output_index = required_index(map, "output_index")?;
        let summary_index = required_index(map, "summary_index")?;
        let item_id = required_string(map, "item_id", event)?;
        let part = reasoning_summary_part(map, event)?;
        let incomplete = match optional_string(map, "status", event)? {
            None => false,
            Some(status) if status == "incomplete" => true,
            Some(_) => return Err(TransformError(format!("{event}.status 必须是 incomplete"))),
        };
        let state = self.reasoning_state(output_index, event)?;
        let Item::Reasoning {
            item_id: expected_id,
            summary_parts,
            content_parts,
            summary_done,
            text,
            incomplete: state_incomplete,
            closed,
            ..
        } = state
        else {
            return Err(TransformError(
                "Responses reasoning summary part 指向非 reasoning item".to_string(),
            ));
        };
        merge_reasoning_id(expected_id, &item_id, event)?;
        if *closed {
            return Err(TransformError(format!("{event} 出现在 reasoning 完成后")));
        }
        if summary_done.contains(&summary_index) {
            if summary_parts.get(&summary_index).map(String::as_str) != Some(part.as_str()) {
                return Err(TransformError(format!("{event} 重复内容不一致")));
            }
            return Ok(());
        }
        replace_reasoning_snapshot(summary_parts, summary_index, &part, event)?;
        summary_done.insert(summary_index);
        *state_incomplete |= incomplete;
        *text = reasoning_text(summary_parts, content_parts);
        Ok(())
    }

    pub(super) fn reasoning_text_delta(&mut self, frame: &Frame) -> Result<(), TransformError> {
        let event = frame
            .event
            .as_deref()
            .ok_or_else(|| TransformError("Responses reasoning delta 缺少事件名".to_string()))?;
        let value = json_data(frame, event)?;
        let map = object(&value, event)?;
        let is_summary = event == "response.reasoning_summary_text.delta";
        let fields = if is_summary {
            &[
                "type",
                "item_id",
                "output_index",
                "summary_index",
                "delta",
                "sequence_number",
            ][..]
        } else {
            &[
                "type",
                "item_id",
                "output_index",
                "content_index",
                "delta",
                "sequence_number",
            ][..]
        };
        allowed(map, fields, event)?;
        if required_string(map, "type", event)? != event {
            return Err(TransformError(format!("{event}.type 无效")));
        }
        let output_index = required_index(map, "output_index")?;
        let part_index = required_index(
            map,
            if is_summary {
                "summary_index"
            } else {
                "content_index"
            },
        )?;
        let item_id = required_string(map, "item_id", event)?;
        let delta = string_value(map, "delta", event)?;
        let state = self.reasoning_state(output_index, event)?;
        let Item::Reasoning {
            item_id: expected_id,
            summary_parts,
            content_parts,
            summary_done,
            content_done,
            text,
            closed,
            ..
        } = state
        else {
            return Err(TransformError(
                "Responses reasoning text 指向非 reasoning item".to_string(),
            ));
        };
        merge_reasoning_id(expected_id, &item_id, event)?;
        if *closed {
            return Err(TransformError(format!("{event} 出现在 reasoning 完成后")));
        }
        if is_summary {
            if summary_done.contains(&part_index) {
                return Err(TransformError(format!(
                    "{event} 出现在 reasoning part 完成后"
                )));
            }
            summary_parts
                .entry(part_index)
                .or_default()
                .push_str(&delta);
        } else {
            if content_done.contains(&part_index) {
                return Err(TransformError(format!(
                    "{event} 出现在 reasoning part 完成后"
                )));
            }
            content_parts
                .entry(part_index)
                .or_default()
                .push_str(&delta);
        }
        *text = reasoning_text(summary_parts, content_parts);
        Ok(())
    }

    pub(super) fn reasoning_text_done(&mut self, frame: &Frame) -> Result<(), TransformError> {
        let event = frame
            .event
            .as_deref()
            .ok_or_else(|| TransformError("Responses reasoning done 缺少事件名".to_string()))?;
        let value = json_data(frame, event)?;
        let map = object(&value, event)?;
        let is_summary = event == "response.reasoning_summary_text.done";
        let fields = if is_summary {
            &[
                "type",
                "item_id",
                "output_index",
                "summary_index",
                "text",
                "sequence_number",
            ][..]
        } else {
            &[
                "type",
                "item_id",
                "output_index",
                "content_index",
                "text",
                "sequence_number",
            ][..]
        };
        allowed(map, fields, event)?;
        if required_string(map, "type", event)? != event {
            return Err(TransformError(format!("{event}.type 无效")));
        }
        let output_index = required_index(map, "output_index")?;
        let part_index = required_index(
            map,
            if is_summary {
                "summary_index"
            } else {
                "content_index"
            },
        )?;
        let item_id = required_string(map, "item_id", event)?;
        let text_value = string_value(map, "text", event)?;
        let state = self.reasoning_state(output_index, event)?;
        let Item::Reasoning {
            item_id: expected_id,
            summary_parts,
            content_parts,
            summary_done,
            content_done,
            text,
            closed,
            ..
        } = state
        else {
            return Err(TransformError(
                "Responses reasoning text 指向非 reasoning item".to_string(),
            ));
        };
        merge_reasoning_id(expected_id, &item_id, event)?;
        if *closed {
            return Err(TransformError(format!("{event} 出现在 reasoning 完成后")));
        }
        if is_summary {
            if summary_done.contains(&part_index) {
                if summary_parts.get(&part_index).map(String::as_str) != Some(text_value.as_str()) {
                    return Err(TransformError(format!("{event} 重复内容不一致")));
                }
                return Ok(());
            }
            replace_reasoning_snapshot(summary_parts, part_index, &text_value, event)?;
            summary_done.insert(part_index);
        } else {
            if content_done.contains(&part_index) {
                if content_parts.get(&part_index).map(String::as_str) != Some(text_value.as_str()) {
                    return Err(TransformError(format!("{event} 重复内容不一致")));
                }
                return Ok(());
            }
            replace_reasoning_snapshot(content_parts, part_index, &text_value, event)?;
            content_done.insert(part_index);
        }
        *text = reasoning_text(summary_parts, content_parts);
        Ok(())
    }

    fn reasoning_state(
        &mut self,
        output_index: u64,
        event: &str,
    ) -> Result<&mut Item, TransformError> {
        let state = self
            .items
            .get_mut(&output_index)
            .ok_or_else(|| TransformError(format!("{event} 找不到 output item")))?;
        if !matches!(state, Item::Reasoning { .. }) {
            return Err(TransformError(format!("{event} 指向非 reasoning item")));
        }
        Ok(state)
    }
}

pub(super) fn finish_reasoning_item(
    state: &mut Item,
    item: &Map<String, Value>,
    output: &mut Vec<u8>,
    transport: Option<&ReasoningTransport>,
) -> Result<(), TransformError> {
    let Item::Reasoning {
        content_index,
        item_id,
        text,
        summary_parts,
        content_parts,
        encrypted_content,
        continuation,
        incomplete,
        emitted,
        closed,
        item_done,
        ..
    } = state
    else {
        unreachable!("reasoning item state checked by caller")
    };
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
    if required_string(item, "type", "Responses reasoning item")? != "reasoning" {
        return Err(TransformError(
            "Responses output item done 类型与 reasoning 不一致".to_string(),
        ));
    }
    validate_reasoning_status(item, "Responses reasoning item")?;
    merge_event_item_id(
        item_id,
        Some(required_string(item, "id", "Responses reasoning item")?),
        "Responses reasoning item",
    )?;
    let final_summary = parse_reasoning_summary(
        item.get("summary")
            .ok_or_else(|| TransformError("Responses reasoning item 缺少 summary".to_string()))?,
        "Responses reasoning item",
    )?;
    let final_content = item
        .get("content")
        .map(|value| parse_reasoning_content(value, "Responses reasoning item"))
        .transpose()?
        .unwrap_or_default();
    if *summary_parts != final_summary || *content_parts != final_content {
        return Err(TransformError(
            "Responses reasoning 完整内容与流式事件不一致".to_string(),
        ));
    }
    if *incomplete {
        return Err(TransformError(
            "Responses reasoning 在完成事件中标记为 incomplete".to_string(),
        ));
    }
    let final_encrypted =
        optional_nullable_string(item, "encrypted_content", "Responses reasoning item")?;
    if let (Some(current), Some(incoming)) = (encrypted_content.as_ref(), final_encrypted.as_ref())
    {
        if current != incoming {
            return Err(TransformError(
                "Responses reasoning encrypted_content 在流中变化".to_string(),
            ));
        }
    }
    if let Some(incoming) = final_encrypted {
        *encrypted_content = Some(incoming);
    }
    if *item_done {
        if let (Some(current), Some(incoming)) = (encrypted_content.as_ref(), continuation.as_ref())
        {
            if current != incoming {
                return Err(TransformError(
                    "Responses reasoning output item done 重复内容不一致".to_string(),
                ));
            }
        }
        return Ok(());
    }
    let resolved = resolve_reasoning_continuation(
        encrypted_content.as_deref(),
        &reasoning_text(summary_parts, content_parts),
        transport,
    )?;
    *text = reasoning_text(summary_parts, content_parts);
    *continuation = Some(resolved);
    *item_done = true;
    emit_reasoning_block(
        *content_index,
        continuation.as_deref().expect("set above"),
        emitted,
        closed,
        output,
    );
    Ok(())
}

pub(super) fn finish_reasoning_expected(
    state: &mut Item,
    expected: &Reasoning,
    output: &mut Vec<u8>,
) -> Result<(), TransformError> {
    let Item::Reasoning {
        content_index,
        text,
        encrypted_content,
        continuation,
        emitted,
        closed,
        item_done,
        ..
    } = state
    else {
        unreachable!("reasoning item state checked by caller")
    };
    if *item_done {
        let matches_existing = if encrypted_content.is_some() {
            continuation
                .as_deref()
                .is_some_and(|value| value == expected.continuation)
        } else {
            *text == expected.content
        };
        if !matches_existing {
            return Err(TransformError(
                "Responses 完整 reasoning 与流式输出不一致".to_string(),
            ));
        }
        return Ok(());
    }
    if encrypted_content.is_some() {
        if encrypted_content.as_deref() != Some(expected.continuation.as_str()) {
            return Err(TransformError(
                "Responses reasoning encrypted_content 与完整响应不一致".to_string(),
            ));
        }
    } else if !text.is_empty() && *text != expected.content {
        return Err(TransformError(
            "Responses 完整 reasoning 与流式摘要不一致".to_string(),
        ));
    }
    *text = expected.content.clone();
    *continuation = Some(expected.continuation.clone());
    *item_done = true;
    emit_reasoning_block(
        *content_index,
        expected.continuation.as_str(),
        emitted,
        closed,
        output,
    );
    Ok(())
}

fn reasoning_summary_part(
    map: &Map<String, Value>,
    context: &str,
) -> Result<String, TransformError> {
    let part = object(
        map.get("part")
            .ok_or_else(|| TransformError(format!("{context} 缺少 part")))?,
        &format!("{context}.part"),
    )?;
    allowed(part, &["type", "text"], "Responses reasoning summary part")?;
    if required_string(part, "type", "Responses reasoning summary part")? != "summary_text" {
        return Err(TransformError(
            "Responses reasoning summary part.type 必须是 summary_text".to_string(),
        ));
    }
    string_value(part, "text", "Responses reasoning summary part")
}

fn merge_reasoning_id(
    expected: &mut Option<String>,
    incoming: &str,
    context: &str,
) -> Result<(), TransformError> {
    match expected {
        Some(current) if current != incoming => Err(TransformError(format!(
            "{context}.item_id 与 output item 不一致"
        ))),
        Some(_) => Ok(()),
        slot @ None => {
            *slot = Some(incoming.to_string());
            Ok(())
        }
    }
}

fn merge_reasoning_snapshot(
    parts: &mut BTreeMap<u64, String>,
    index: u64,
    incoming: &str,
    context: &str,
) -> Result<(), TransformError> {
    let current = parts.entry(index).or_default();
    if current.is_empty() {
        current.push_str(incoming);
    } else if let Some(suffix) = incoming.strip_prefix(current.as_str()) {
        current.push_str(suffix);
    } else if current != incoming {
        return Err(TransformError(format!("{context} 内容与已发送事件不一致")));
    }
    Ok(())
}

fn replace_reasoning_snapshot(
    parts: &mut BTreeMap<u64, String>,
    index: u64,
    incoming: &str,
    context: &str,
) -> Result<(), TransformError> {
    let current = parts.entry(index).or_default();
    if let Some(suffix) = incoming.strip_prefix(current.as_str()) {
        current.push_str(suffix);
        return Ok(());
    }
    if current != incoming {
        return Err(TransformError(format!("{context} 内容与已发送事件不一致")));
    }
    Ok(())
}

fn resolve_reasoning_continuation(
    encrypted_content: Option<&str>,
    text: &str,
    transport: Option<&ReasoningTransport>,
) -> Result<String, TransformError> {
    let transport =
        transport.ok_or_else(|| TransformError("当前转换缺少本机推理续接通道".to_string()))?;
    if let Some(encrypted_content) = encrypted_content {
        if encrypted_content.is_empty() {
            return Err(TransformError(
                "Responses reasoning.encrypted_content 不能为空".to_string(),
            ));
        }
        return transport
            .from_continuation(encrypted_content.to_string())
            .map(|reasoning| reasoning.continuation);
    }
    if text.is_empty() {
        return Err(TransformError(
            "Responses reasoning 缺少可转换的 summary 或 content".to_string(),
        ));
    }
    transport
        .from_chat_content(text.to_string())
        .map(|reasoning| reasoning.continuation)
}

fn emit_reasoning_block(
    content_index: u64,
    continuation: &str,
    emitted: &mut bool,
    closed: &mut bool,
    output: &mut Vec<u8>,
) {
    if !*emitted {
        append_event(
            output,
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": content_index,
                "content_block": { "type": "redacted_thinking", "data": continuation },
            }),
        );
        *emitted = true;
    }
    if !*closed {
        append_event(
            output,
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": content_index }),
        );
        *closed = true;
    }
}
