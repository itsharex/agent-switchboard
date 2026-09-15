//! Local session discovery, controlled resume, and explicit deletion for the
//! two clients Agent Switchboard owns.
//!
//! Discovery and transcript reads never index or modify client data. An
//! explicit resume request only starts the client's fixed CLI command in a
//! new terminal after resolving an approved local session source. An
//! explicit delete permanently removes the one local record the backend
//! itself resolved from the approved roots.

pub(crate) mod parser;
mod codex_titles;
mod resume;

use asb_core::contracts::AppKind;
use parser::{parse_session, read_messages};
use resume::{launch_terminal, resume_arguments, resume_command};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const TITLE_LIMIT: usize = 120;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub app: AppKind,
    pub session_id: String,
    pub title: String,
    pub summary: String,
    pub project_dir: Option<String>,
    pub created_at: Option<String>,
    pub last_active_at: Option<String>,
    pub resume_command: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    pub at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIssue {
    pub app: AppKind,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionScan {
    pub sessions: Vec<SessionMeta>,
    pub issues: Vec<SessionIssue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionResume {
    pub command: String,
    pub used_project_dir: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDeleteRequest {
    pub app: AppKind,
    pub session_id: String,
}

/// One line of a batch deletion: the record either went away or the reason
/// it stayed, never a silent skip.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDeleteOutcome {
    pub app: AppKind,
    pub session_id: String,
    pub deleted: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
struct SessionSource {
    meta: SessionMeta,
    path: PathBuf,
}

pub fn scan_sessions() -> SessionScan {
    let (roots, titles) = match session_roots().and_then(|roots| Ok((roots, codex_titles()?))) {
        Ok(found) => found,
        Err(message) => {
            return SessionScan {
                sessions: Vec::new(),
                issues: vec![SessionIssue {
                    app: AppKind::Codex,
                    message,
                }],
            }
        }
    };
    scan_session_roots(&roots, &titles)
}

pub fn load_messages(app: AppKind, session_id: &str) -> Result<Vec<SessionMessage>, String> {
    let source = resolve_session(app, session_id)?;
    read_messages(&source.path)
}

/// Opens the selected local session in a new terminal window. The renderer
/// supplies only the supported client plus a validated session id; the source
/// path and working directory stay resolved inside this module.
pub fn resume_session(app: AppKind, session_id: &str) -> Result<SessionResume, String> {
    let source = resolve_session(app, session_id)?;
    let project_dir = source
        .meta
        .project_dir
        .as_deref()
        .filter(|path| Path::new(path).is_dir());
    launch_terminal(&resume_arguments(app, session_id), project_dir)?;

    Ok(SessionResume {
        command: resume_command(app, session_id),
        used_project_dir: project_dir.is_some(),
    })
}

/// Permanently removes the local session record resolved from the approved
/// roots. The renderer supplies only the supported client plus a validated
/// session id; the resolved path never crosses the IPC boundary. Claude Code
/// keeps an optional directory named after the transcript stem beside the
/// record; it belongs to this session's identity and is removed with it.
pub fn delete_session(app: AppKind, session_id: &str) -> Result<(), String> {
    let source = resolve_session(app, session_id)?;
    remove_session_source(app, &source.path)
}

/// Deletes several records against one scan. Each request resolves and
/// fails on its own line; a repeated (client, id) pair is answered once.
/// Only an unreadable root set aborts the batch before anything is removed.
pub fn delete_sessions(requests: &[SessionDeleteRequest]) -> Result<Vec<SessionDeleteOutcome>, String> {
    let (sources, issues) = scan_sources()?;
    Ok(delete_resolved(&sources, &issues, requests))
}

fn delete_resolved(
    sources: &[SessionSource],
    issues: &[SessionIssue],
    requests: &[SessionDeleteRequest],
) -> Vec<SessionDeleteOutcome> {
    let mut seen = HashSet::new();
    let mut outcomes = Vec::with_capacity(requests.len());
    for request in requests {
        if !seen.insert((request.app, request.session_id.clone())) {
            continue;
        }
        let result = locate(sources, issues, request.app, &request.session_id)
            .and_then(|source| remove_session_source(request.app, &source.path));
        outcomes.push(SessionDeleteOutcome {
            app: request.app,
            session_id: request.session_id.clone(),
            deleted: result.is_ok(),
            error: result.err(),
        });
    }
    outcomes
}

fn remove_session_source(app: AppKind, path: &Path) -> Result<(), String> {
    if app == AppKind::Claude {
        if let Some(stem) = path.file_stem() {
            let sidecar = path.with_file_name(stem);
            if sidecar.is_dir() {
                fs::remove_dir_all(&sidecar).map_err(|error| {
                    format!("无法删除会话附属目录 {}: {error}", sidecar.display())
                })?;
            }
        }
    }
    fs::remove_file(path).map_err(|error| format!("无法删除会话记录 {}: {error}", path.display()))
}

fn resolve_session(app: AppKind, session_id: &str) -> Result<SessionSource, String> {
    let (sources, issues) = scan_sources()?;
    locate(&sources, &issues, app, session_id).cloned()
}

fn locate<'a>(
    sources: &'a [SessionSource],
    issues: &[SessionIssue],
    app: AppKind,
    session_id: &str,
) -> Result<&'a SessionSource, String> {
    if !parser::valid_session_id(session_id) {
        return Err("会话 ID 无效".to_string());
    }
    sources
        .iter()
        .find(|source| source.meta.app == app && source.meta.session_id == session_id)
        .ok_or_else(|| {
            issues
                .first()
                .map(|issue| issue.message.clone())
                .unwrap_or_else(|| "找不到指定会话；请刷新会话列表后重试".to_string())
        })
}

fn scan_sources() -> Result<(Vec<SessionSource>, Vec<SessionIssue>), String> {
    Ok(scan_session_source_roots(
        &session_roots()?,
        &HashMap::new(),
    ))
}

/// The Codex root every Codex-owned sidecar (sessions, rename index, state
/// database) hangs off; honours the same `CODEX_HOME` as the config file.
fn codex_root() -> Result<PathBuf, String> {
    let codex = crate::local_state::LocalState::user_config_path(AppKind::Codex)?;
    codex
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "Codex 配置目录无效".to_string())
}

/// Thread renames are display data only; identity resolution never needs
/// them, so only the list scan pays for reading the index and database.
fn codex_titles() -> Result<HashMap<String, String>, String> {
    Ok(codex_titles::load(&codex_root()?))
}

/// The only approved local JSONL roots. Both session browsing and usage
/// aggregation consume this list so neither feature accepts a renderer path.
pub(crate) fn session_roots() -> Result<Vec<(AppKind, PathBuf)>, String> {
    let claude = crate::local_state::LocalState::user_config_path(AppKind::Claude)?;
    let claude_root = claude.parent().ok_or("Claude 配置目录无效")?;
    let mut roots = crate::local_state::codex_paths::session_roots(&codex_root()?)
        .into_iter()
        .map(|path| (AppKind::Codex, path))
        .collect::<Vec<_>>();
    roots.push((AppKind::Claude, claude_root.join("projects")));
    Ok(roots)
}

fn scan_session_roots(
    roots: &[(AppKind, PathBuf)],
    codex_titles: &HashMap<String, String>,
) -> SessionScan {
    let (mut sources, issues) = scan_session_source_roots(roots, codex_titles);
    sources.sort_by(|left, right| {
        right
            .meta
            .last_active_at
            .as_deref()
            .unwrap_or("")
            .cmp(left.meta.last_active_at.as_deref().unwrap_or(""))
    });
    SessionScan {
        sessions: sources.into_iter().map(|source| source.meta).collect(),
        issues,
    }
}

fn scan_session_source_roots(
    roots: &[(AppKind, PathBuf)],
    codex_titles: &HashMap<String, String>,
) -> (Vec<SessionSource>, Vec<SessionIssue>) {
    let mut sources = Vec::new();
    let mut issues = Vec::new();
    let mut seen = HashSet::new();

    for (app, root) in roots {
        match collect_session_jsonl_files(root) {
            Ok(paths) => {
                for path in paths {
                    if let Ok(mut meta) = parse_session(*app, &path) {
                        // A Codex rename lives beside the transcript, not in
                        // it; it replaces the derived title but never the
                        // summary, which stays the first real prompt.
                        if *app == AppKind::Codex {
                            if let Some(title) = codex_titles.get(&meta.session_id) {
                                meta.title = parser::clamp_title(title);
                            }
                        }
                        // Roots are deliberately ordered: a live Codex
                        // session owns its identity ahead of an archived
                        // copy. The same identity cannot appear twice in a
                        // scan, so list rendering and session resolution use
                        // the identical source contract.
                        if seen.insert((meta.app, meta.session_id.clone())) {
                            sources.push(SessionSource { meta, path });
                        }
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => issues.push(SessionIssue {
                app: *app,
                message: format!("无法读取{}会话目录", (*app).label()),
            }),
        }
    }

    (sources, issues)
}

/// Recursively finds client-owned JSONL session records without following
/// symlinks. Callers receive no input path from the renderer.
pub(crate) fn collect_session_jsonl_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let mut pending = vec![root.to_path_buf()];

    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
            {
                paths.push(entry.path());
            }
        }
    }

    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(path, content).expect("write session");
    }

    #[test]
    fn scans_supported_sources_without_exposing_paths() {
        let temp = tempdir().expect("temp");
        let codex = temp.path().join("codex").join("a.jsonl");
        let claude = temp.path().join("claude").join("b.jsonl");
        write(
            &codex,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"codex-123\",\"cwd\":\"C:/work/relay\",\"timestamp\":\"2026-08-01T10:00:00Z\"}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":\"修复会话页面\"}}\n"
            ),
        );
        write(
            &claude,
            concat!(
                "{\"type\":\"user\",\"sessionId\":\"claude-456\",\"cwd\":\"C:/work/claude\",\"timestamp\":\"2026-08-02T10:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"整理历史\"}}\n",
                "{\"type\":\"custom-title\",\"customTitle\":\"归档整理\"}\n"
            ),
        );

        let scan = scan_session_roots(
            &[
                (AppKind::Codex, temp.path().join("codex")),
                (AppKind::Claude, temp.path().join("claude")),
            ],
            &HashMap::new(),
        );

        assert!(scan.issues.is_empty());
        assert_eq!(scan.sessions.len(), 2);
        let claude = scan
            .sessions
            .iter()
            .find(|session| session.app == AppKind::Claude)
            .expect("Claude session");
        let codex = scan
            .sessions
            .iter()
            .find(|session| session.app == AppKind::Codex)
            .expect("Codex session");
        assert_eq!(claude.title, "归档整理");
        assert_eq!(claude.resume_command, "claude --resume claude-456");
        assert_eq!(codex.resume_command, "codex resume codex-123");
    }

    #[test]
    fn duplicate_session_ids_keep_the_first_configured_source() {
        let temp = tempdir().expect("temp");
        let active_root = temp.path().join("codex").join("sessions");
        let archived_root = temp.path().join("codex").join("archived_sessions");
        write(
            &active_root.join("active.jsonl"),
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"codex-duplicate\",\"cwd\":\"C:/work/relay\",\"timestamp\":\"2026-08-01T10:00:00Z\"}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":\"活动会话\"}}\n"
            ),
        );
        write(
            &archived_root.join("archived.jsonl"),
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"codex-duplicate\",\"cwd\":\"C:/work/relay\",\"timestamp\":\"2026-07-01T10:00:00Z\"}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":\"归档副本\"}}\n"
            ),
        );

        let scan = scan_session_roots(
            &[
                (AppKind::Codex, active_root.clone()),
                (AppKind::Codex, archived_root),
            ],
            &HashMap::new(),
        );

        assert_eq!(scan.sessions.len(), 1);
        assert_eq!(scan.sessions[0].session_id, "codex-duplicate");
        assert_eq!(scan.sessions[0].title, "活动会话");

        // A rename recorded beside the transcript replaces the derived title
        // for the Codex record only; the summary keeps the first prompt.
        let renamed = scan_session_roots(
            &[(AppKind::Codex, active_root)],
            &HashMap::from([(
                "codex-duplicate".to_string(),
                "  部署线程（已重命名）  ".to_string(),
            )]),
        );
        assert_eq!(renamed.sessions[0].title, "部署线程（已重命名）");
        assert_eq!(renamed.sessions[0].summary, "活动会话");
    }

    #[test]
    fn extracts_messages_only_when_a_record_has_a_role_and_content() {
        let temp = tempdir().expect("temp");
        let path = temp.path().join("session.jsonl");
        write(
            &path,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"codex-123\"}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"你好\"}]}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"已完成\"}]}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"shell\"}}\n"
            ),
        );

        let messages = read_messages(&path).expect("read messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "你好");
        assert_eq!(messages[1].role, "assistant");
    }

    #[test]
    fn invalid_session_ids_are_not_made_resumable() {
        assert!(!parser::valid_session_id("session; shutdown"));
        assert!(!parser::valid_session_id(""));
        assert!(parser::valid_session_id(
            "019f4b74-c859-7e72-bb0c-9f83347954fb"
        ));
    }

    #[test]
    fn metadata_records_beyond_the_line_cap_are_ignored() {
        let temp = tempdir().expect("temp");
        let mut content = String::new();
        for _ in 0..(parser::METADATA_LINE_LIMIT + 1) {
            content.push_str(
                "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"噪声\"}}\n",
            );
        }
        content.push_str(
            "{\"sessionId\":\"late-1\",\"cwd\":\"C:/work/late\",\"message\":{\"role\":\"user\",\"content\":\"很晚才出现\"}}\n",
        );
        write(&temp.path().join("late.jsonl"), &content);

        let scan = scan_session_roots(
            &[(AppKind::Claude, temp.path().to_path_buf())],
            &HashMap::new(),
        );
        assert!(
            scan.sessions.is_empty(),
            "会话 ID 在限界之后才出现时不应被收录"
        );
    }

    #[test]
    fn removes_the_record_and_claude_sidecar_without_touching_siblings() {
        let temp = tempdir().expect("temp");
        let record = temp.path().join("session-1.jsonl");
        let sidecar = temp.path().join("session-1");
        write(&record, "{\"type\":\"user\"}\n");
        fs::create_dir_all(sidecar.join("parts")).expect("create sidecar");
        write(&sidecar.join("parts").join("chunk.bin"), "payload");
        let unrelated = temp.path().join("session-2.jsonl");
        write(&unrelated, "{\"type\":\"user\"}\n");

        remove_session_source(AppKind::Claude, &record).expect("delete claude record");
        assert!(!record.exists(), "会话记录应被删除");
        assert!(!sidecar.exists(), "同名附属目录应随会话删除");
        assert!(unrelated.exists(), "无关记录必须保留");

        let codex_record = temp.path().join("rollout-1.jsonl");
        let codex_sidecar = temp.path().join("rollout-1");
        write(&codex_record, "{\"type\":\"session_meta\"}\n");
        fs::create_dir_all(&codex_sidecar).expect("create codex sidecar");

        remove_session_source(AppKind::Codex, &codex_record).expect("delete codex record");
        assert!(!codex_record.exists(), "会话记录应被删除");
        assert!(codex_sidecar.exists(), "Codex 不清理同名目录");
    }

    #[test]
    fn deleting_a_missing_record_reports_failure() {
        let temp = tempdir().expect("temp");
        let error = remove_session_source(AppKind::Codex, &temp.path().join("gone.jsonl"))
            .expect_err("missing record must fail");
        assert!(error.contains("无法删除会话记录"));
    }

    #[test]
    fn batch_deletion_reports_each_request_and_removes_only_resolved_records() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join("codex").join("sessions");
        for (file, id) in [("one.jsonl", "codex-one"), ("two.jsonl", "codex-two")] {
            write(
                &root.join(file),
                &format!(
                    "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"cwd\":\"C:/work\",\"timestamp\":\"2026-08-01T10:00:00Z\"}}}}\n\
                     {{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":\"问题\"}}}}\n"
                ),
            );
        }
        let (sources, issues) =
            scan_session_source_roots(&[(AppKind::Codex, root.clone())], &HashMap::new());
        let request = |app, id: &str| SessionDeleteRequest {
            app,
            session_id: id.to_string(),
        };
        let outcomes = delete_resolved(
            &sources,
            &issues,
            &[
                request(AppKind::Codex, "codex-one"),
                request(AppKind::Codex, "codex-one"),
                request(AppKind::Codex, "codex-missing"),
                request(AppKind::Claude, "codex-two"),
                request(AppKind::Codex, "bad id;"),
            ],
        );

        assert_eq!(outcomes.len(), 4, "重复请求只回答一次");
        assert!(outcomes[0].deleted && outcomes[0].error.is_none());
        assert!(!outcomes[1].deleted);
        assert!(outcomes[1].error.as_deref().unwrap().contains("找不到指定会话"));
        assert!(
            !outcomes[2].deleted,
            "同一 ID 属于另一客户端时不得跨客户端删除"
        );
        assert_eq!(outcomes[3].error.as_deref(), Some("会话 ID 无效"));
        assert!(!root.join("one.jsonl").exists());
        assert!(root.join("two.jsonl").exists(), "未解析到的请求不得影响其他记录");
    }

    #[test]
    fn resume_arguments_keep_the_client_command_and_id_separate() {
        assert_eq!(
            resume_arguments(AppKind::Codex, "codex-123"),
            ["codex", "resume", "codex-123"]
        );
        assert_eq!(
            resume_arguments(AppKind::Claude, "claude-456"),
            ["claude", "--resume", "claude-456"]
        );
    }

    #[test]
    fn shell_resume_command_keeps_the_executable_and_arguments_separate() {
        assert_eq!(
            resume::shell_command(&["codex", "resume", "codex-123"]),
            "'codex' 'resume' 'codex-123'"
        );
        assert_eq!(
            resume::shell_command(&["claude", "--resume", "id-with-'quote"]),
            "'claude' '--resume' 'id-with-'\\''quote'"
        );
    }
}
