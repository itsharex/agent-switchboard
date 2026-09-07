//! WebSocket handshake primitives: key validation and accept computation.

use super::*;

pub(super) fn websocket_key(request: &Request) -> Result<String, &'static str> {
    let upgrade = single_header(request, "Upgrade").ok_or("WebSocket 缺少 Upgrade 头")?;
    if !header_contains(&upgrade, "websocket") {
        return Err("WebSocket Upgrade 头无效");
    }
    let connection = single_header(request, "Connection").ok_or("WebSocket 缺少 Connection 头")?;
    if !header_contains(&connection, "upgrade") {
        return Err("WebSocket Connection 头无效");
    }
    if single_header(request, "Sec-WebSocket-Version").as_deref() != Some("13") {
        return Err("WebSocket 版本必须为 13");
    }
    let key = single_header(request, "Sec-WebSocket-Key").ok_or("WebSocket 缺少密钥")?;
    if STANDARD
        .decode(key.as_bytes())
        .ok()
        .is_none_or(|decoded| decoded.len() != 16)
    {
        return Err("WebSocket 密钥无效");
    }
    Ok(key)
}

pub(super) fn single_header(request: &Request, name: &'static str) -> Option<String> {
    let mut value = None;
    for header in request.headers() {
        if header.field.equiv(name) {
            if value.replace(header.value.as_str().to_string()).is_some() {
                return None;
            }
        }
    }
    value
}

pub(super) fn header_contains(value: &str, expected: &str) -> bool {
    value
        .split(',')
        .any(|part| part.trim().eq_ignore_ascii_case(expected))
}

pub(super) fn websocket_accept(key: &str) -> String {
    let mut digest = Sha1::new();
    digest.update(key.as_bytes());
    digest.update(WEBSOCKET_ACCEPT_GUID.as_bytes());
    STANDARD.encode(digest.finalize())
}
