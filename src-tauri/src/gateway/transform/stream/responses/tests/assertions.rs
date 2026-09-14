use super::*;

pub(super) fn of_type<'a>(events: &'a [Value], kind: &str) -> Vec<&'a Value> {
    events
        .iter()
        .filter(|event| event["type"] == kind)
        .collect()
}

pub(super) fn assert_success(harness: &Harness, expected: &Value) {
    assert_eq!(of_type(&harness.events, "message_start").len(), 1);
    assert_eq!(of_type(&harness.events, "message_delta").len(), 1);
    assert_eq!(of_type(&harness.events, "message_stop").len(), 1);
    assert!(of_type(&harness.events, "error").is_empty());
    assert_eq!(harness.events.last().unwrap()["type"], "message_stop");
    let mut from_stream = message(&harness.events);
    let mut from_json: Value = serde_json::from_slice(
        &convert_response(
            UpstreamProtocol::Responses,
            UpstreamProtocol::AnthropicMessages,
            &serde_json::to_vec(expected).unwrap(),
            Some(&transport()),
        )
        .unwrap(),
    )
    .unwrap();
    normalize_reasoning(&mut from_stream);
    normalize_reasoning(&mut from_json);
    assert_eq!(from_stream, from_json);
}

fn normalize_reasoning(message: &mut Value) {
    for part in message["content"].as_array_mut().unwrap() {
        if part["type"] == "redacted_thinking" {
            let opened = transport()
                .from_continuation(part["data"].as_str().unwrap().into())
                .unwrap();
            let native = opened.responses_input_item().unwrap();
            assert!(!native.to_string().contains("asb-reasoning-"));
            part["data"] = native;
        }
    }
}

pub(super) fn message(events: &[Value]) -> Value {
    let mut message = of_type(events, "message_start")[0]["message"].clone();
    let mut parts = BTreeMap::new();
    let mut arguments = BTreeMap::<u64, String>::new();
    let mut closed = BTreeSet::new();
    for event in events {
        let index = event["index"].as_u64().unwrap_or(0);
        match event["type"].as_str().unwrap() {
            "content_block_start" => {
                assert_eq!(index, parts.len() as u64, "blocks must follow output order");
                assert_eq!(
                    closed.len(),
                    parts.len(),
                    "Claude blocks must not interleave"
                );
                assert!(parts
                    .insert(index, event["content_block"].clone())
                    .is_none());
            }
            "content_block_delta" => {
                assert!(!closed.contains(&index), "delta after block_stop: {event}");
                apply_delta(
                    parts.get_mut(&index).unwrap(),
                    arguments.entry(index).or_default(),
                    event,
                );
            }
            "content_block_stop" => {
                assert!(parts.contains_key(&index));
                assert!(closed.insert(index), "duplicate block_stop");
                let part = parts.get_mut(&index).unwrap();
                if part["type"] == "tool_use" {
                    if let Some(value) = arguments.get(&index) {
                        part["input"] = serde_json::from_str(value).unwrap();
                    }
                }
            }
            "message_delta" => {
                message["stop_reason"] = event["delta"]["stop_reason"].clone();
                message["stop_sequence"] = event["delta"]["stop_sequence"].clone();
                for (field, value) in event["usage"].as_object().unwrap() {
                    message["usage"][field] = value.clone();
                }
            }
            "message_stop" => assert_eq!(closed.len(), parts.len()),
            "message_start" => {}
            other => panic!("unexpected Claude event {other}"),
        }
    }
    message["content"] = Value::Array(parts.into_values().collect());
    message
}

fn apply_delta(part: &mut Value, arguments: &mut String, event: &Value) {
    match event["delta"]["type"].as_str().unwrap() {
        "text_delta" => {
            let text = format!(
                "{}{}",
                part["text"].as_str().unwrap(),
                event["delta"]["text"].as_str().unwrap()
            );
            part["text"] = json!(text);
        }
        "input_json_delta" => arguments.push_str(event["delta"]["partial_json"].as_str().unwrap()),
        other => panic!("unexpected delta {other}"),
    }
}

pub(super) fn assert_rejected(harness: &mut Harness, result: Result<(), TransformError>) {
    assert!(result.is_err());
    assert!(of_type(&harness.events, "message_stop").is_empty());
    assert!(harness.finish().is_err());
}
