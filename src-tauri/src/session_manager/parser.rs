use super::{SessionMessage, SessionMeta, TITLE_LIMIT};
use asb_core::contracts::AppKind;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::Path;

const SUMMARY_LIMIT: usize = 180;
/// Every metadata field below is first-match-wins, so once all five are
/// collected the remaining lines cannot change the result. This cap bounds
/// the per-file scan cost when some fields never appear; records past the cap
/// are ignored and the title falls back as if they were absent.
pub(super) const METADATA_LINE_LIMIT: usize = 32;

pub(super) fn parse_session(app: AppKind, path: &Path) -> Result<SessionMeta, io::Error> {
    let mut session_id = None;
    let mut project_dir = None;
    let mut created_at = None;
    let mut custom_title = None;
    let mut first_user_message = None;

    let reader = BufReader::new(File::open(path)?);
    for (index, line) in reader.lines().take(METADATA_LINE_LIMIT).enumerate() {
        let line = line?;
        if line.trim().is_empty() { continue; }
        let value = serde_json::from_str::<Value>(&line).map_err(|error| {
            io::Error::new(io::ErrorKind::InvalidData, format!("第 {} 行 JSON 无效: {error}", index + 1))
        })?;
        session_id = session_id.or_else(|| session_id_from(&value));
        project_dir = project_dir.or_else(|| project_dir_from(&value));
        created_at = created_at.or_else(|| timestamp_from(&value));
        custom_title = custom_title.or_else(|| custom_title_from(&value));
        if first_user_message.is_none() {
            first_user_message = message_from(&value)
                .filter(|message| message.role == "user")
                .map(|message| message.content)
                .filter(|content| useful_user_content(content));
        }
        if session_id.is_some()
            && project_dir.is_some()
            && created_at.is_some()
            && custom_title.is_some()
            && first_user_message.is_some()
        {
            break;
        }
    }

    let session_id = session_id
        .filter(|id| valid_session_id(id))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing session id"))?;
    let title = custom_title
        .or_else(|| first_user_message.clone())
        .or_else(|| project_dir.as_deref().and_then(project_name))
        .unwrap_or_else(|| session_id.clone());
    let last_active_at = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(system_time_iso);

    Ok(SessionMeta {
        app,
        resume_command: super::resume::resume_command(app, &session_id),
        session_id,
        title: clamp_text(&title, TITLE_LIMIT),
        summary: clamp_text(
            first_user_message.as_deref().unwrap_or(&title),
            SUMMARY_LIMIT,
        ),
        project_dir,
        created_at,
        last_active_at,
        alias: None,
        pinned: false,
        tags: Vec::new(),
    })
}

pub(crate) fn session_id_in_session_file(path: &Path) -> io::Result<Option<String>> {
    let reader = BufReader::new(File::open(path)?);
    for line in reader.lines().take(METADATA_LINE_LIMIT) {
        let line = line?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(session_id) = session_id_from(&value).filter(|id| valid_session_id(id)) {
            return Ok(Some(session_id));
        }
    }
    Ok(None)
}

pub(super) fn read_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    let file = File::open(path)
        .map_err(|error| format!("无法读取会话记录 {}: {error}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut messages = Vec::new();
    let mut prefix = Sha256::new();
    let mut line = String::new();
    let mut line_number = 0;
    loop {
        line.clear();
        let read = reader.read_line(&mut line)
            .map_err(|error| format!("无法读取 {} 第 {} 行: {error}", path.display(), line_number + 1))?;
        if read == 0 { break; }
        line_number += 1;
        // Hash the complete prefix: appending preserves IDs; edits cannot
        // redirect an old search hit to a different occurrence of a message.
        prefix.update(line.trim_end_matches(['\r', '\n']).as_bytes());
        prefix.update(b"\n");
        if line.trim().is_empty() { continue; }
        let value = serde_json::from_str::<Value>(&line)
            .map_err(|error| format!("会话 {} 第 {line_number} 行 JSON 无效: {error}", path.display()))?;
        if let Some(mut message) = message_from(&value) {
            message.id = format!("{:x}", prefix.clone().finalize());
            messages.push(message);
        }
    }
    Ok(messages)
}

fn message_from(value: &Value) -> Option<SessionMessage> {
    let envelope = if value.get("type").and_then(Value::as_str) == Some("response_item") {
        let payload = value.get("payload").unwrap_or(value);
        if matches!(payload.get("type").and_then(Value::as_str),
            Some("function_call_output" | "custom_tool_call_output")) {
            let content = payload.get("output").and_then(content_text)?;
            if content.trim().is_empty() { return None; }
            return Some(SessionMessage {
                id: String::new(), role: "tool".into(), content, at: timestamp_from(value),
            });
        }
        payload
    } else {
        value
    };
    let message = envelope.get("message").unwrap_or(envelope);
    let role = message
        .get("role")
        .or_else(|| envelope.get("role"))
        .and_then(Value::as_str)
        .or_else(|| match envelope.get("type").and_then(Value::as_str) {
            Some("user") => Some("user"),
            Some("assistant") => Some("assistant"),
            Some("system") => Some("system"),
            _ => None,
        })?;
    let content = message
        .get("content")
        .or_else(|| envelope.get("content"))
        .and_then(content_text)?;
    if content.trim().is_empty() {
        return None;
    }
    Some(SessionMessage {
        id: String::new(),
        role: role.to_string(),
        content,
        at: timestamp_from(value),
    })
}

fn content_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(content_text)
                .collect::<Vec<_>>()
                .join("\n");
            (!text.is_empty()).then_some(text)
        }
        Value::Object(object) => object
            .get("text")
            .or_else(|| object.get("content"))
            .or_else(|| object.get("value"))
            .and_then(content_text),
        _ => None,
    }
}

fn session_id_from(value: &Value) -> Option<String> {
    let payload = value.get("payload").unwrap_or(value);
    value
        .get("sessionId")
        .or_else(|| value.get("session_id"))
        .or_else(|| payload.get("id"))
        .or_else(|| payload.get("sessionId"))
        .or_else(|| payload.get("session_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn project_dir_from(value: &Value) -> Option<String> {
    let payload = value.get("payload").unwrap_or(value);
    value
        .get("cwd")
        .or_else(|| payload.get("cwd"))
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
}

fn timestamp_from(value: &Value) -> Option<String> {
    let payload = value.get("payload").unwrap_or(value);
    value
        .get("timestamp")
        .or_else(|| payload.get("timestamp"))
        .and_then(Value::as_str)
        .filter(|timestamp| !timestamp.is_empty())
        .map(str::to_string)
}

fn custom_title_from(value: &Value) -> Option<String> {
    value
        .get("customTitle")
        .or_else(|| value.get("custom_title"))
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .map(str::to_string)
}

pub(super) fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Applies the list-title limit to a title sourced outside the transcript.
pub(super) fn clamp_title(text: &str) -> String {
    clamp_text(text, TITLE_LIMIT)
}

fn clamp_text(text: &str, limit: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= limit {
        return text;
    }
    let mut shortened = text.chars().take(limit).collect::<String>();
    shortened.push('…');
    shortened
}

fn useful_user_content(content: &str) -> bool {
    !content.starts_with("<local-command-caveat>") && !content.starts_with("<command-name>")
}

fn project_name(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn system_time_iso(time: std::time::SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339()
}
