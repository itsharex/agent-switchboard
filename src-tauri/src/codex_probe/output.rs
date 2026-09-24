//! CLI event identity and exact-session usage. Neither stderr nor unrelated
//! sessions can provide an answer, model or token count for a probe.
use crate::codex_metering::{summarize_token_usage, SessionTokenSummary};
use serde_json::Value;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(super) struct ProbeEvents {
    pub session_id: String,
    pub completed: bool,
    pub failure: Option<String>,
}

pub(super) fn parse_events(output: &str) -> Result<ProbeEvents, String> {
    let mut session_id = None;
    let mut completed = false;
    let mut failure = None;
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let event: Value = serde_json::from_str(line)
            .map_err(|_| "Codex CLI 未返回有效的 JSON 事件".to_string())?;
        match event.get("type").and_then(Value::as_str) {
            Some("thread.started") => {
                let id = event.get("thread_id").and_then(Value::as_str)
                    .and_then(|id| uuid::Uuid::parse_str(id).ok())
                    .ok_or_else(|| "Codex CLI 返回了无效的会话 ID".to_string())?
                    .hyphenated().to_string();
                if session_id.as_ref().is_some_and(|previous| previous != &id) {
                    return Err("Codex CLI 返回了多个会话 ID".to_string());
                }
                session_id = Some(id);
            }
            Some("turn.completed") => completed = true,
            Some("turn.failed") => {
                failure = Some(diagnostic(event.pointer("/error/message")
                    .and_then(Value::as_str).unwrap_or("Codex 执行失败")));
            }
            _ => {}
        }
    }
    Ok(ProbeEvents {
        session_id: session_id.ok_or_else(|| "Codex CLI 未返回会话 ID".to_string())?,
        completed,
        failure,
    })
}

pub(super) fn diagnostic(message: &str) -> String {
    let scrubbed = asb_core::adapter::scrub_message(message.to_string());
    let tail: String = scrubbed.chars().rev().take(1000).collect();
    tail.chars().rev().collect::<String>().trim().to_string()
}

pub(super) fn read_usage(session_id: &str) -> Result<SessionTokenSummary, String> {
    let root = crate::session_manager::codex_root()?;
    let mut attempts = 0;
    loop {
        let result = read_session_usage(&root, session_id);
        attempts += 1;
        if result.is_ok() || attempts == 3 { return result; }
        std::thread::sleep(Duration::from_millis(400));
    }
}

fn read_session_usage(root: &Path, session_id: &str) -> Result<SessionTokenSummary, String> {
    let mut matched: Option<PathBuf> = None;
    for session_root in crate::local_state::codex_paths::session_roots(root) {
        if !session_root.exists() { continue; }
        let paths = crate::session_manager::collect_session_jsonl_files(asb_core::contracts::AppKind::Codex, &session_root)
            .map_err(|_| "无法读取 Codex 会话目录".to_string())?;
        for path in paths {
            // The filename narrows the candidates; metadata establishes identity.
            if !path.file_stem().and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(session_id)) { continue; }
            let id = crate::session_manager::parser::session_id_in_session_file(&path)
                .map_err(|_| "无法读取检测会话标识".to_string())?;
            if id.as_deref() != Some(session_id) { continue; }
            if matched.replace(path).is_some() {
                return Err("检测会话存在多份记录，无法确定用量".to_string());
            }
        }
    }
    let path = matched.ok_or_else(|| "未找到本次检测会话的用量记录".to_string())?;
    let file = std::fs::File::open(path)
        .map_err(|_| "无法读取检测会话用量".to_string())?;
    summarize_token_usage(BufReader::new(file))
}

#[cfg(test)]
mod tests;
