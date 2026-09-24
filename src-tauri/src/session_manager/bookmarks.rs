use super::{organization, parser::read_messages, resolve_session, SessionMessage, SessionMeta};
use asb_core::contracts::AppKind;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

const SCHEMA: &str = "CREATE TABLE bookmarks (
    id TEXT PRIMARY KEY NOT NULL,
    app TEXT NOT NULL CHECK(app IN ('codex', 'claude')),
    session_id TEXT NOT NULL,
    session_title TEXT NOT NULL,
    project_dir TEXT,
    message_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
    content TEXT NOT NULL CHECK(length(content) > 0),
    at TEXT,
    saved_at TEXT NOT NULL,
    UNIQUE(app, session_id, message_id)
) STRICT";
const COLUMNS: &str = "id, app, session_id, session_title, project_dir, message_id, role, content, at, saved_at";
static SCHEMA_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBookmark {
    pub id: String,
    pub app: AppKind,
    pub session_id: String,
    pub session_title: String,
    pub project_dir: Option<String>,
    pub message_id: String,
    pub role: String,
    pub content: String,
    pub at: Option<String>,
    pub saved_at: String,
}

pub fn list_bookmarks(root: &Path) -> Result<Vec<SessionBookmark>, String> {
    if !database_path(root).exists() { return Ok(Vec::new()); }
    let connection = open_store(root)?;
    let mut statement = connection.prepare(&format!(
        "SELECT {COLUMNS} FROM bookmarks ORDER BY saved_at DESC, id ASC"
    )).map_err(database_error)?;
    let rows = statement.query_map([], from_row).map_err(database_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
}

pub fn save_bookmark(
    root: &Path,
    app: AppKind,
    session_id: &str,
    message_id: &str,
) -> Result<SessionBookmark, String> {
    let mut source = resolve_session(app, session_id)?;
    organization::apply(&mut source.meta, &organization::load(root)?);
    let message = read_messages(&source.path)?.into_iter()
        .find(|message| message.id == message_id)
        .ok_or("原消息已改变或不存在；请重新打开会话后收藏")?;
    save_snapshot(root, &source.meta, &message)
}

fn save_snapshot(root: &Path, meta: &SessionMeta, message: &SessionMessage) -> Result<SessionBookmark, String> {
    if !matches!(message.role.as_str(), "user" | "assistant") {
        return Err("只能收藏用户或助手消息".into());
    }
    let identity = format!("{}\0{}\0{}", meta.app.dir_name(), meta.session_id, message.id);
    let id = format!("{:x}", Sha256::digest(identity.as_bytes()));
    let mut connection = open_store(root)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    transaction.execute(
        "INSERT INTO bookmarks (id, app, session_id, session_title, project_dir, message_id, role, content, at, saved_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(app, session_id, message_id) DO NOTHING",
        params![id, meta.app.dir_name(), meta.session_id, meta.alias.as_deref().unwrap_or(&meta.title), meta.project_dir,
            message.id, message.role, message.content, message.at, Utc::now().to_rfc3339()],
    ).map_err(database_error)?;
    let bookmark = transaction.query_row(
        &format!("SELECT {COLUMNS} FROM bookmarks WHERE id = ?1"), [&id], from_row,
    ).map_err(database_error)?;
    transaction.commit().map_err(database_error)?;
    Ok(bookmark)
}

pub fn delete_bookmark(root: &Path, id: &str) -> Result<(), String> {
    if !database_path(root).exists() { return Ok(()); }
    let connection = open_store(root)?;
    connection.execute("DELETE FROM bookmarks WHERE id = ?1", [id]).map_err(database_error)?;
    Ok(())
}

fn database_path(root: &Path) -> PathBuf {
    root.join("state").join("sessions").join("collections.sqlite3")
}

fn open_store(root: &Path) -> Result<Connection, String> {
    // The desktop has one process; serialize existence checks through schema
    // commit so another command cannot mistake a new file for a damaged DB.
    let _guard = SCHEMA_GATE.lock().map_err(|_| "收藏库初始化锁不可用")?;
    let path = database_path(root);
    let existed = path.exists();
    std::fs::create_dir_all(path.parent().ok_or("收藏目录无效")?)
        .map_err(|error| format!("无法创建收藏目录: {error}"))?;
    let mut connection = Connection::open(&path).map_err(database_error)?;
    connection.busy_timeout(Duration::from_secs(5)).map_err(database_error)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let version: i64 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(database_error)?;
    let tables: i64 = transaction.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        [], |row| row.get(0),
    ).map_err(database_error)?;
    if !existed && version == 0 && tables == 0 {
        transaction.execute_batch(SCHEMA).map_err(database_error)?;
        transaction.pragma_update(None, "user_version", 1).map_err(database_error)?;
    } else {
        let schema: Option<String> = transaction.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'bookmarks'",
            [], |row| row.get(0),
        ).optional().map_err(database_error)?;
        if version != 1 || tables != 1 || schema.as_deref() != Some(SCHEMA) {
            return Err(format!("收藏库结构无效，未修改原文件：{}", path.display()));
        }
    }
    transaction.commit().map_err(database_error)?;
    Ok(connection)
}

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionBookmark> {
    let app: String = row.get(1)?;
    let app = match app.as_str() {
        "codex" => AppKind::Codex,
        "claude" => AppKind::Claude,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(SessionBookmark { id: row.get(0)?, app, session_id: row.get(2)?,
        session_title: row.get(3)?, project_dir: row.get(4)?, message_id: row.get(5)?,
        role: row.get(6)?, content: row.get(7)?, at: row.get(8)?, saved_at: row.get(9)? })
}

fn database_error(error: rusqlite::Error) -> String {
    format!("收藏库操作失败: {error}")
}

#[cfg(test)]
mod tests;
