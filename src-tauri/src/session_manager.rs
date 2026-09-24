//! Local session discovery, controlled resume, and explicit deletion for the
//! two clients Agent Switchboard owns.
//!
//! Discovery and transcript reads never index or modify client data. An
//! explicit resume request only starts the client's fixed CLI command in a
//! new terminal after resolving an approved local session source. An
//! explicit delete permanently removes the one local record the backend
//! itself resolved from the approved roots.

pub(crate) mod codex_titles;
pub(crate) mod parser;
mod resume;
mod bookmarks;
mod search;
mod export;
mod organization;

pub use bookmarks::{delete_bookmark, list_bookmarks, save_bookmark, SessionBookmark};
pub use export::export_markdown;
pub use search::{search_sessions, SessionSearchPage, SessionSearchRequest};
pub use organization::{update_session_organization, SessionOrganizationChange};

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
    pub alias: Option<String>,
    pub pinned: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    pub id: String,
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
pub struct SessionResume {
    pub command: String,
    pub used_project_dir: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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

pub fn load_metadata(root: &Path, app: AppKind, session_id: &str) -> Result<SessionMeta, String> {
    let mut meta = resolve_session(app, session_id)?.meta;
    organization::apply(&mut meta, &organization::load(root)?);
    Ok(meta)
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
pub fn delete_session(root: &Path, app: AppKind, session_id: &str) -> Result<(), String> {
    let source = resolve_session(app, session_id)?;
    organization::delete_with_source(root, app, session_id, || remove_session_source(app, &source.path))
}

/// Deletes several records against one scan. Each request resolves and
/// fails on its own line; a repeated (client, id) pair is answered once.
/// Only an unreadable root set aborts the batch before anything is removed.
pub fn delete_sessions(
    root: &Path,
    requests: &[SessionDeleteRequest],
) -> Result<Vec<SessionDeleteOutcome>, String> {
    let (sources, issues) = scan_sources()?;
    Ok(delete_resolved(root, &sources, &issues, requests))
}

fn delete_resolved(
    root: &Path,
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
        let source = locate(sources, issues, request.app, &request.session_id);
        let result = source.as_ref().map_err(|error| error.clone()).and_then(|source| {
            organization::delete_with_source(root, request.app, &request.session_id,
                || remove_session_source(request.app, &source.path))
        });
        let deleted = result.is_ok() || source.is_ok_and(|source| matches!(source.path.try_exists(), Ok(false)));
        outcomes.push(SessionDeleteOutcome {
            app: request.app,
            session_id: request.session_id.clone(),
            deleted,
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
        &codex_titles()?,
    ))
}

/// The Codex root every Codex-owned sidecar (sessions, rename index, state
/// database) hangs off; honours the same `CODEX_HOME` as the config file.
pub(crate) fn codex_root() -> Result<PathBuf, String> {
    let codex = crate::local_state::LocalState::user_config_path(AppKind::Codex)?;
    codex
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "Codex 配置目录无效".to_string())
}

/// Thread renames are display data; every exported title uses the same
/// current name as the session list.
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

fn scan_session_source_roots(
    roots: &[(AppKind, PathBuf)],
    codex_titles: &HashMap<String, String>,
) -> (Vec<SessionSource>, Vec<SessionIssue>) {
    let mut sources = Vec::new();
    let mut issues = Vec::new();
    let mut seen = HashSet::new();

    for (app, root) in roots {
        match collect_session_jsonl_files(*app, root) {
            Ok(paths) => {
                for path in paths {
                    match parse_session(*app, &path) {
                        Ok(mut meta) => {
                            // A rename replaces the derived title; the summary
                            // remains the first real prompt.
                            if *app == AppKind::Codex {
                                if let Some(title) = codex_titles.get(&meta.session_id) {
                                    meta.title = parser::clamp_title(title);
                                }
                            }
                            // Ordered roots give a live session precedence
                            // over an archived copy with the same identity.
                            if seen.insert((meta.app, meta.session_id.clone())) {
                                sources.push(SessionSource { meta, path });
                            }
                        }
                        Err(error) => issues.push(SessionIssue {
                            app: *app,
                            message: format!("无法读取会话 {}: {error}", path.display()),
                        }),
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && !root.exists() => {}
            Err(error) => issues.push(SessionIssue {
                app: *app,
                message: format!("无法读取{}会话目录 {}: {error}", (*app).label(), root.display()),
            }),
        }
    }

    (sources, issues)
}

/// Finds only the client-owned transcript layout. Codex rollouts are nested
/// by date and require recursion; Claude transcripts are direct JSONL children
/// of each encoded project directory. Internal Claude subagent journals are
/// separate artifacts and never enter the session contract.
pub(crate) fn collect_session_jsonl_files(
    app: AppKind,
    root: &Path,
) -> io::Result<Vec<PathBuf>> {
    match app {
        AppKind::Codex => collect_recursive_jsonl_files(root),
        AppKind::Claude => collect_claude_project_sessions(root),
    }
}

fn collect_recursive_jsonl_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() { continue; }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if is_jsonl_file(&entry, &file_type) {
                paths.push(entry.path());
            }
        }
    }
    paths.sort();
    Ok(paths)
}

fn collect_claude_project_sessions(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for project in fs::read_dir(root)? {
        let project = project?;
        let file_type = project.file_type()?;
        if file_type.is_symlink() || !file_type.is_dir() { continue; }
        for entry in fs::read_dir(project.path())? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if !file_type.is_symlink() && is_jsonl_file(&entry, &file_type) {
                paths.push(entry.path());
            }
        }
    }
    paths.sort();
    Ok(paths)
}

fn is_jsonl_file(entry: &fs::DirEntry, file_type: &fs::FileType) -> bool {
    file_type.is_file()
        && entry.path().extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
}

#[cfg(test)]
mod tests;
