//! Reasoning item handling for the Responses to Anthropic stream bridge.

use super::*;
mod events;

pub(super) fn validate_reasoning_status(
    item: &Map<String, Value>,
    context: &str,
) -> Result<(), TransformError> {
    match item.get("status") {
        None | Some(Value::Null) => Ok(()),
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
    if value.is_null() {
        return Ok(BTreeMap::new());
    }
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
    if value.is_null() {
        return Ok(BTreeMap::new());
    }
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
        let parsed = events::part_event(frame, false)?;
        let event = parsed.event;
        let Item::Reasoning {
            item_id,
            summary_parts,
            content_parts,
            summary_done,
            text,
            closed,
            ..
        } = self.reasoning_state(parsed.output_index, event)?
        else {
            unreachable!("checked reasoning state");
        };
        merge_reasoning_id(item_id, &parsed.item_id, event)?;
        if *closed || summary_done.contains(&parsed.part_index) {
            return Err(TransformError(format!(
                "{event} 出现在 reasoning part 完成后"
            )));
        }
        merge_reasoning_snapshot(summary_parts, parsed.part_index, &parsed.text, event)?;
        *text = reasoning_text(summary_parts, content_parts);
        Ok(())
    }

    pub(super) fn reasoning_part_done(&mut self, frame: &Frame) -> Result<(), TransformError> {
        let parsed = events::part_event(frame, true)?;
        let event = parsed.event;
        let Item::Reasoning {
            item_id,
            summary_parts,
            content_parts,
            summary_done,
            text,
            incomplete,
            closed,
            ..
        } = self.reasoning_state(parsed.output_index, event)?
        else {
            unreachable!("checked reasoning state");
        };
        merge_reasoning_id(item_id, &parsed.item_id, event)?;
        if *closed {
            return Err(TransformError(format!("{event} 出现在 reasoning 完成后")));
        }
        if summary_done.contains(&parsed.part_index) {
            if summary_parts.get(&parsed.part_index).map(String::as_str)
                != Some(parsed.text.as_str())
            {
                return Err(TransformError(format!("{event} 重复内容不一致")));
            }
            return Ok(());
        }
        replace_reasoning_snapshot(summary_parts, parsed.part_index, &parsed.text, event)?;
        summary_done.insert(parsed.part_index);
        *incomplete |= parsed.incomplete;
        *text = reasoning_text(summary_parts, content_parts);
        Ok(())
    }

    pub(super) fn reasoning_text_delta(&mut self, frame: &Frame) -> Result<(), TransformError> {
        let parsed = events::text_event(frame, false)?;
        let event = parsed.event;
        let Item::Reasoning {
            item_id,
            summary_parts,
            content_parts,
            summary_done,
            content_done,
            text,
            closed,
            ..
        } = self.reasoning_state(parsed.output_index, event)?
        else {
            unreachable!("checked reasoning state");
        };
        merge_reasoning_id(item_id, &parsed.item_id, event)?;
        if *closed {
            return Err(TransformError(format!("{event} 出现在 reasoning 完成后")));
        }
        if parsed.is_summary {
            append_delta(summary_parts, summary_done, &parsed)?;
        } else {
            append_delta(content_parts, content_done, &parsed)?;
        }
        *text = reasoning_text(summary_parts, content_parts);
        Ok(())
    }

    pub(super) fn reasoning_text_done(&mut self, frame: &Frame) -> Result<(), TransformError> {
        let parsed = events::text_event(frame, true)?;
        let event = parsed.event;
        let Item::Reasoning {
            item_id,
            summary_parts,
            content_parts,
            summary_done,
            content_done,
            text,
            closed,
            ..
        } = self.reasoning_state(parsed.output_index, event)?
        else {
            unreachable!("checked reasoning state");
        };
        merge_reasoning_id(item_id, &parsed.item_id, event)?;
        if *closed {
            return Err(TransformError(format!("{event} 出现在 reasoning 完成后")));
        }
        if parsed.is_summary {
            finish_text_part(summary_parts, summary_done, &parsed)?;
        } else {
            finish_text_part(content_parts, content_done, &parsed)?;
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

fn append_delta(
    parts: &mut BTreeMap<u64, String>,
    done: &BTreeSet<u64>,
    parsed: &events::ReasoningEvent<'_>,
) -> Result<(), TransformError> {
    if done.contains(&parsed.part_index) {
        return Err(TransformError(format!(
            "{} 出现在 reasoning part 完成后",
            parsed.event
        )));
    }
    parts
        .entry(parsed.part_index)
        .or_default()
        .push_str(&parsed.text);
    Ok(())
}

fn finish_text_part(
    parts: &mut BTreeMap<u64, String>,
    done: &mut BTreeSet<u64>,
    parsed: &events::ReasoningEvent<'_>,
) -> Result<(), TransformError> {
    if done.contains(&parsed.part_index) {
        if parts.get(&parsed.part_index).map(String::as_str) != Some(parsed.text.as_str()) {
            return Err(TransformError(format!("{} 重复内容不一致", parsed.event)));
        }
        return Ok(());
    }
    replace_reasoning_snapshot(parts, parsed.part_index, &parsed.text, parsed.event)?;
    done.insert(parsed.part_index);
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
