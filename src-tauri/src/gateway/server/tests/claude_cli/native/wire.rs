use super::*;
use base64::Engine;
use std::io::Cursor;
fn events(text: &str) -> Vec<Value> {
    vec![
        json!({"type":"message_start","message":{"id":"native-msg","type":"message","role":"assistant","model":"claude-sonnet-4-6","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":3}}),
        json!({"type":"message_stop"}),
    ]
}
pub(super) fn response(kind: Kind, stream: bool, text: &str) -> Response<Cursor<Vec<u8>>> {
    if !stream {
        return Response::from_data(json!({"id":"native-json","type":"message","role":"assistant","model":"claude-sonnet-4-6","content":[{"type":"text","text":text}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":3}}).to_string().into_bytes()).with_header(content_type("application/json"));
    }
    let events = events(text);
    let bytes = if kind == Kind::Bedrock {
        events.iter().flat_map(aws_event).collect()
    } else {
        events
            .iter()
            .map(|v| format!("event: {}\ndata: {v}\n\n", v["type"].as_str().unwrap()))
            .collect::<String>()
            .into_bytes()
    };
    Response::from_data(bytes).with_header(content_type(if kind == Kind::Bedrock {
        "application/vnd.amazon.eventstream"
    } else {
        "text/event-stream"
    }))
}
fn aws_event(value: &Value) -> Vec<u8> {
    let payload =
        json!({"bytes":base64::engine::general_purpose::STANDARD.encode(value.to_string())})
            .to_string();
    let mut headers = Vec::new();
    for (key, value) in [
        (":event-type", "chunk"),
        (":message-type", "event"),
        (":content-type", "application/json"),
    ] {
        headers.push(key.len() as u8);
        headers.extend(key.as_bytes());
        headers.push(7);
        headers.extend((value.len() as u16).to_be_bytes());
        headers.extend(value.as_bytes());
    }
    let mut bytes = Vec::new();
    bytes.extend((16 + headers.len() as u32 + payload.len() as u32).to_be_bytes());
    bytes.extend((headers.len() as u32).to_be_bytes());
    bytes.extend(crc32(&bytes).to_be_bytes());
    bytes.extend(headers);
    bytes.extend(payload.as_bytes());
    bytes.extend(crc32(&bytes).to_be_bytes());
    bytes
}
fn crc32(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!0u32, |mut crc, byte| {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
        }
        crc
    })
}
