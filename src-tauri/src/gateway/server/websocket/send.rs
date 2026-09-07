//! Client-facing WebSocket frame writers.

use super::*;

pub(super) fn send_created_and_completed<S>(socket: &mut WebSocket<S>, response: &Value) -> bool
where
    S: Read + Write,
{
    let Some(id) = response.get("id").and_then(Value::as_str) else {
        return send_failed(socket, "gateway_error", "响应缺少 id");
    };
    let Some(model) = response.get("model").and_then(Value::as_str) else {
        return send_failed(socket, "gateway_error", "响应缺少 model");
    };
    send_value(
        socket,
        &json!({
            "type": "response.created",
            "response": {
                "id": id,
                "object": "response",
                "status": "in_progress",
                "model": model,
                "output": [],
            },
        }),
    ) && send_value(
        socket,
        &json!({ "type": "response.completed", "response": response }),
    )
}

pub(super) fn send_failed<S>(socket: &mut WebSocket<S>, code: &str, message: &str) -> bool
where
    S: Read + Write,
{
    send_value(
        socket,
        &json!({
            "type": "response.failed",
            "response": {
                "object": "response",
                "status": "failed",
                "error": { "code": code, "message": message },
            },
        }),
    )
}

pub(super) fn send_value<S>(socket: &mut WebSocket<S>, value: &Value) -> bool
where
    S: Read + Write,
{
    serde_json::to_string(value)
        .ok()
        .and_then(|text| socket.send(Message::Text(text)).ok())
        .is_some()
}
