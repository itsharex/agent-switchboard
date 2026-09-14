//! Validate event envelopes before handing their snapshots to shared parsers.

use super::*;

pub(super) fn named_frame(mut frame: Frame) -> Result<Frame, TransformError> {
    let value = json_data(&frame, "Responses SSE data")?;
    let map = object(&value, "Responses SSE data")?;
    let kind = required_string(map, "type", "Responses SSE data")?;
    if frame.event.as_deref().is_some_and(|event| event != kind) {
        return Err(TransformError(
            "Responses SSE event and data.type disagree".into(),
        ));
    }
    frame.event = Some(kind);
    Ok(frame)
}

pub(super) fn failure(frame: &Frame) -> Result<TransformError, TransformError> {
    let event = frame.event.as_deref().expect("named event");
    let value = json_data(frame, event)?;
    let map = object(&value, event)?;
    let response = match map.get("response") {
        Some(response) => object(response, event)?,
        None => map,
    };
    Ok(response_lifecycle::upstream_error(response, event))
}

pub(super) fn terminal_response(
    frame: &Frame,
) -> Result<(Value, ResponsesTerminal), TransformError> {
    let event = frame.event.as_deref().expect("named terminal event");
    let value = json_data(frame, event)?;
    let map = object(&value, event)?;
    response_lifecycle::ensure_no_error(map, event)?;
    allowed(map, &["type", "sequence_number", "response"], event)?;
    let response = map
        .get("response")
        .ok_or_else(|| TransformError(format!("{event} requires response")))?;
    let mut response = object(response, event)?.clone();
    let expected_status = if event == "response.incomplete" {
        "incomplete"
    } else {
        "completed"
    };
    // A named terminal event is authoritative only when the body omitted status.
    response
        .entry("status")
        .or_insert_with(|| json!(expected_status));
    let terminal = ResponsesTerminal::parse(&response)?;
    if response.get("status").and_then(Value::as_str) != Some(expected_status) {
        return Err(TransformError(format!(
            "{event} conflicts with response.status"
        )));
    }
    Ok((Value::Object(response), terminal))
}

impl ResponsesToAnthropic {
    pub(super) fn defer_incomplete_item(
        &mut self,
        index: u64,
        item: &Map<String, Value>,
    ) -> Result<bool, TransformError> {
        if !self.items.contains_key(&index) {
            return Err(TransformError(
                "Responses output item done has no starting item".into(),
            ));
        }
        let snapshot = Value::Object(item.clone());
        if let Some(previous) = self.incomplete_items.get(&index) {
            if previous != &snapshot {
                return Err(TransformError(
                    "Responses incomplete item changed after output_item.done".into(),
                ));
            }
            return Ok(true);
        }
        if item.get("status").and_then(Value::as_str) != Some("incomplete") {
            return Ok(false);
        }
        // Do not close an incomplete payload before the response explains why.
        ResponsesTerminal::MaxTokens.validate_item(item)?;
        self.incomplete_items.insert(index, snapshot);
        Ok(true)
    }

    pub(super) fn validate_incomplete_items(
        &self,
        output: &[Value],
        terminal: ResponsesTerminal,
    ) -> Result<(), TransformError> {
        for (index, expected) in &self.incomplete_items {
            if terminal != ResponsesTerminal::MaxTokens
                || output.get(*index as usize) != Some(expected)
            {
                return Err(TransformError(
                    "Responses terminal output conflicts with an incomplete item".into(),
                ));
            }
        }
        Ok(())
    }
}
