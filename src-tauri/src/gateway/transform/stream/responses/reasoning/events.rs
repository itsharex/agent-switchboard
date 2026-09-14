use super::*;

pub(super) struct ReasoningEvent<'a> {
    pub(super) event: &'a str,
    pub(super) output_index: u64,
    pub(super) part_index: u64,
    pub(super) item_id: String,
    pub(super) text: String,
    pub(super) is_summary: bool,
    pub(super) incomplete: bool,
}

pub(super) fn part_event(
    frame: &Frame,
    done: bool,
) -> Result<ReasoningEvent<'static>, TransformError> {
    let event = if done {
        "response.reasoning_summary_part.done"
    } else {
        "response.reasoning_summary_part.added"
    };
    let value = json_data(frame, event)?;
    let map = object(&value, event)?;
    let mut fields = vec![
        "type",
        "item_id",
        "output_index",
        "summary_index",
        "part",
        "sequence_number",
    ];
    if done {
        fields.push("status");
    }
    allowed(map, &fields, event)?;
    if required_string(map, "type", event)? != event {
        return Err(TransformError(format!("{event}.type 无效")));
    }
    let incomplete = match optional_string(map, "status", event)? {
        None => false,
        Some(status) if status == "incomplete" => true,
        Some(_) => return Err(TransformError(format!("{event}.status 必须是 incomplete"))),
    };
    Ok(ReasoningEvent {
        event,
        output_index: required_index(map, "output_index")?,
        part_index: required_index(map, "summary_index")?,
        item_id: required_string(map, "item_id", event)?,
        text: reasoning_summary_part(map, event)?,
        is_summary: true,
        incomplete,
    })
}

pub(super) fn text_event(frame: &Frame, done: bool) -> Result<ReasoningEvent<'_>, TransformError> {
    let phase = if done { "done" } else { "delta" };
    let event = frame
        .event
        .as_deref()
        .ok_or_else(|| TransformError(format!("Responses reasoning {phase} 缺少事件名")))?;
    let is_summary = event
        == if done {
            "response.reasoning_summary_text.done"
        } else {
            "response.reasoning_summary_text.delta"
        };
    let index_key = if is_summary {
        "summary_index"
    } else {
        "content_index"
    };
    let text_key = if done { "text" } else { "delta" };
    let value = json_data(frame, event)?;
    let map = object(&value, event)?;
    allowed(
        map,
        &[
            "type",
            "item_id",
            "output_index",
            index_key,
            text_key,
            "sequence_number",
        ],
        event,
    )?;
    if required_string(map, "type", event)? != event {
        return Err(TransformError(format!("{event}.type 无效")));
    }
    Ok(ReasoningEvent {
        event,
        output_index: required_index(map, "output_index")?,
        part_index: required_index(map, index_key)?,
        item_id: required_string(map, "item_id", event)?,
        text: string_value(map, text_key, event)?,
        is_summary,
        incomplete: false,
    })
}
