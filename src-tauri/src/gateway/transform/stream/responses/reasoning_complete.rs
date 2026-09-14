use super::*;
use crate::gateway::transform::Reasoning;

pub(super) fn finish_reasoning_item(
    state: &mut Item,
    item: &Map<String, Value>,
    output: &mut Vec<u8>,
    transport: Option<&ReasoningTransport>,
) -> Result<(), TransformError> {
    let transport =
        transport.ok_or_else(|| TransformError("当前转换缺少本机推理续接通道".into()))?;
    let expected = transport.from_responses_item(Value::Object(item.clone()))?;
    finish_reasoning_expected(state, &expected, output)
}

pub(super) fn finish_reasoning_expected(
    state: &mut Item,
    expected: &Reasoning,
    output: &mut Vec<u8>,
) -> Result<(), TransformError> {
    let incoming = expected.responses_input_item()?;
    let final_summary = parts(&incoming, "summary")?;
    let final_content = parts(&incoming, "content")?;
    validate_native_snapshot(state, &incoming, &final_summary, &final_content)?;
    let Item::Reasoning {
        content_index,
        item_id,
        text,
        summary_parts,
        content_parts,
        encrypted_content,
        native_item,
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
        return Ok(());
    }
    *summary_parts = final_summary;
    *content_parts = final_content;
    *encrypted_content = native_encrypted(&incoming).map(str::to_string);
    *item_id = incoming
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string);
    *native_item = Some(incoming);
    *text = expected.content.clone();
    *continuation = Some(expected.continuation.clone());
    *item_done = true;
    emit_reasoning_block(
        *content_index,
        &expected.continuation,
        emitted,
        closed,
        output,
    );
    Ok(())
}

fn validate_native_snapshot(
    state: &Item,
    incoming: &Value,
    final_summary: &BTreeMap<u64, String>,
    final_content: &BTreeMap<u64, String>,
) -> Result<(), TransformError> {
    let Item::Reasoning {
        item_id,
        summary_parts,
        content_parts,
        summary_done,
        content_done,
        encrypted_content,
        native_item,
        incomplete,
        item_done,
        ..
    } = state
    else {
        unreachable!("reasoning item state checked by caller")
    };
    if *incomplete {
        return Err(TransformError(
            "Responses reasoning part was marked incomplete".into(),
        ));
    }
    if item_id
        .as_deref()
        .is_some_and(|id| incoming.get("id").and_then(Value::as_str) != Some(id))
    {
        return Err(TransformError(
            "Responses terminal reasoning item.id differs from its stream".into(),
        ));
    }
    validate_parts(summary_parts, summary_done, final_summary)?;
    validate_parts(content_parts, content_done, final_content)?;
    if encrypted_content
        .as_deref()
        .filter(|value| !value.is_empty())
        .is_some_and(|value| Some(value) != native_encrypted(incoming))
    {
        return Err(TransformError(
            "Responses reasoning encrypted_content changed in the stream".into(),
        ));
    }
    if *item_done && native_item.as_ref() != Some(incoming) {
        return Err(TransformError(
            "Responses terminal reasoning differs from output_item.done".into(),
        ));
    }
    Ok(())
}

fn native_encrypted(item: &Value) -> Option<&str> {
    item.get("encrypted_content")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

fn parts(item: &Value, field: &str) -> Result<BTreeMap<u64, String>, TransformError> {
    let Some(value) = item.get(field).filter(|value| !value.is_null()) else {
        return Ok(BTreeMap::new());
    };
    if field == "summary" {
        parse_reasoning_summary(value, "Responses reasoning item")
    } else {
        parse_reasoning_content(value, "Responses reasoning item")
    }
}

fn validate_parts(
    observed: &BTreeMap<u64, String>,
    finished: &BTreeSet<u64>,
    expected: &BTreeMap<u64, String>,
) -> Result<(), TransformError> {
    for (index, text) in observed {
        // An empty placeholder carries no generated text and may be omitted.
        if text.is_empty() && !expected.contains_key(index) {
            continue;
        }
        let matches = expected.get(index).is_some_and(|final_text| {
            if finished.contains(index) {
                final_text == text
            } else {
                final_text.starts_with(text)
            }
        });
        if !matches {
            return Err(TransformError(
                "Responses reasoning 完整内容与流式事件不一致".into(),
            ));
        }
    }
    Ok(())
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
                "type":"content_block_start", "index":content_index,
                "content_block":{"type":"redacted_thinking","data":continuation},
            }),
        );
        *emitted = true;
    }
    if !*closed {
        append_event(
            output,
            "content_block_stop",
            json!({"type":"content_block_stop","index":content_index}),
        );
        *closed = true;
    }
}
