//! Claude managed upstream request policy. No Codex client configuration is read or written.

use super::ResolvedAccount;
use asb_core::{claude_auth::ClaudeAuthProvider, contracts::UpstreamProtocol};
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(crate) fn headers(account: &ResolvedAccount, headers: &mut HeaderMap) {
    headers.remove("x-api-key");
    if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", account.access_token)) {
        headers.insert("authorization", value);
    }
    match account.provider {
        ClaudeAuthProvider::GithubCopilot => {
            for (name, value) in [
                ("editor-version", "vscode/1.110.1"),
                ("editor-plugin-version", "copilot-chat/0.38.2"),
                ("copilot-integration-id", "vscode-chat"),
                ("x-github-api-version", "2025-10-01"),
                ("user-agent", "GitHubCopilotChat/0.38.2"),
                ("openai-intent", "conversation-agent"),
                ("x-interaction-type", "conversation-agent"),
            ] {
                headers.insert(name, HeaderValue::from_static(value));
            }
        }
        ClaudeAuthProvider::CodexOauth => {
            headers.insert(
                "openai-beta",
                HeaderValue::from_static("responses=experimental"),
            );
            headers.insert("originator", HeaderValue::from_static("codex_cli_rs"));
            if let Some(id) = &account.upstream_account_id {
                if let Ok(value) = HeaderValue::from_str(id) {
                    headers.insert("chatgpt-account-id", value);
                }
            }
        }
        ClaudeAuthProvider::XaiOauth => {
            headers.insert(
                "user-agent",
                HeaderValue::from_static("Agent-Switchboard-Claude"),
            );
        }
    }
}

/// Returns whether this upstream requires SSE even for a non-streaming client.
pub(crate) fn body(
    account: &ResolvedAccount,
    protocol: UpstreamProtocol,
    bytes: &mut Vec<u8>,
) -> Result<bool, String> {
    account.provider.validate_protocol(protocol)?;
    if account.provider == ClaudeAuthProvider::GithubCopilot {
        let original = bytes.clone();
        super::copilot_model::apply(bytes, &original)?;
    }
    let mut value: Value = serde_json::from_slice(bytes).map_err(|_| "Claude 上游请求不是 JSON")?;
    let object = value.as_object_mut().ok_or("Claude 上游请求必须是对象")?;
    if account.provider == ClaudeAuthProvider::CodexOauth {
        object.insert("stream".into(), json!(true));
        object.insert("store".into(), json!(false));
        object.entry("instructions").or_insert(json!(""));
        for key in [
            "max_output_tokens",
            "max_tokens",
            "temperature",
            "top_p",
            "user",
        ] {
            object.remove(key);
        }
        let include = object.entry("include").or_insert(json!([]));
        let include = include
            .as_array_mut()
            .ok_or("Claude Responses include 必须是数组")?;
        if !include
            .iter()
            .any(|item| item == "reasoning.encrypted_content")
        {
            include.push(json!("reasoning.encrypted_content"));
        }
    }
    if account.provider == ClaudeAuthProvider::XaiOauth {
        if let Some(tools) = object.get_mut("tools").and_then(Value::as_array_mut) {
            for tool in tools {
                if let Some(tool) = tool.as_object_mut() {
                    tool.remove("strict");
                }
            }
        }
    }
    *bytes = serde_json::to_vec(&value).map_err(|_| "Claude 上游请求无法编码")?;
    Ok(account.provider == ClaudeAuthProvider::CodexOauth)
}

/// A default-account change must not make another account's opaque reasoning replayable.
pub(crate) fn continuation_key(key: [u8; 32], account: &ResolvedAccount) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"asb-claude-account-reasoning-v1\0");
    hash.update(key);
    hash.update(account.account_id.as_bytes());
    if let Some(id) = &account.upstream_account_id {
        hash.update(b"\0");
        hash.update(id.as_bytes());
    }
    hash.finalize().into()
}

pub(crate) fn completed_response(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "Claude 托管上游 SSE 不是 UTF-8")?;
    let text = text.replace("\r\n", "\n");
    let mut completed = None;
    for frame in text.split("\n\n") {
        let data = frame
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(|line| line.strip_prefix(' ').unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let event: Value =
            serde_json::from_str(&data).map_err(|_| "Claude 托管上游 SSE JSON 无效")?;
        match event.get("type").and_then(Value::as_str) {
            Some("error" | "response.failed") => {
                return Err("Claude 托管上游在完成前返回错误".into())
            }
            Some("response.completed" | "response.incomplete") => {
                if completed.is_some() {
                    return Err("Claude 托管上游返回了重复的终止事件".into());
                }
                let response = event
                    .get("response")
                    .filter(|value| value.is_object())
                    .ok_or("Claude 托管上游终止事件缺少完整 response")?;
                completed =
                    Some(serde_json::to_vec(response).map_err(|_| "Claude 完成响应无法编码")?);
            }
            _ => {}
        }
    }
    completed.ok_or_else(|| "Claude 托管上游 SSE 截断，缺少终止事件".into())
}

pub(crate) fn request_headers(account: &ResolvedAccount, target: &mut HeaderMap, body: &[u8]) {
    headers(account, target);
    if account.provider != ClaudeAuthProvider::GithubCopilot {
        return;
    }
    let value: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let last = value
        .get("messages")
        .or_else(|| value.get("input"))
        .and_then(Value::as_array)
        .and_then(|values| values.last());
    let continuation = last.is_some_and(|message| {
        matches!(
            message.get("role").and_then(Value::as_str),
            Some("assistant" | "tool")
        ) || matches!(
            message.get("type").and_then(Value::as_str),
            Some("function_call_output" | "custom_tool_call_output")
        ) || message
            .get("content")
            .and_then(Value::as_array)
            .is_some_and(|content| {
                content
                    .iter()
                    .any(|part| part.get("type").and_then(Value::as_str) == Some("tool_result"))
            })
    });
    target.insert(
        "x-initiator",
        HeaderValue::from_static(if continuation { "agent" } else { "user" }),
    );
    if let Ok(id) = HeaderValue::from_str(&uuid::Uuid::new_v4().to_string()) {
        target.insert("x-request-id", id.clone());
        target.insert("x-agent-task-id", id);
    }
}
