use super::{validate_value, Organization, Organizations};
use asb_core::contracts::AppKind;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::{Path, PathBuf};
use std::time::Duration;

static SCHEMA_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
const SCHEMA: &str = "CREATE TABLE organization (
    app TEXT NOT NULL CHECK(app IN ('codex', 'claude')),
    session_id TEXT NOT NULL,
    alias TEXT,
    pinned INTEGER NOT NULL CHECK(pinned IN (0, 1)),
    tags TEXT NOT NULL CHECK(json_valid(tags) AND json_type(tags) = 'array'),
    PRIMARY KEY(app, session_id)
) STRICT";

fn path(root: &Path) -> PathBuf {
    root.join("state").join("sessions").join("organization.sqlite3")
}

pub(super) fn exists(root: &Path) -> Result<bool, String> {
    path(root).try_exists().map_err(|error| format!("无法检查会话整理库：{error}"))
}

pub(super) fn open(root: &Path) -> Result<Connection, String> {
    let _guard = SCHEMA_GATE.lock().map_err(|_| "会话整理库初始化锁不可用")?;
    let existed = exists(root)?;
    let path = path(root);
    std::fs::create_dir_all(path.parent().ok_or("会话整理库目录无效")?)
        .map_err(|error| format!("无法创建会话整理目录：{error}"))?;
    let mut connection = Connection::open(&path).map_err(error)?;
    connection.busy_timeout(Duration::from_secs(5)).map_err(error)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(error)?;
    let version: i64 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0)).map_err(error)?;
    let objects: i64 = transaction.query_row(
        "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'", [], |row| row.get(0),
    ).map_err(error)?;
    if !existed && version == 0 && objects == 0 {
        transaction.execute_batch(SCHEMA).map_err(error)?;
        transaction.pragma_update(None, "user_version", 1).map_err(error)?;
    } else {
        let sql: Option<String> = transaction.query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='organization'", [], |row| row.get(0),
        ).optional().map_err(error)?;
        if version != 1 || objects != 1 || sql.as_deref() != Some(SCHEMA) {
            return Err(format!("会话整理库结构无效，未修改原文件：{}", path.display()));
        }
    }
    transaction.commit().map_err(error)?;
    Ok(connection)
}

pub(super) fn read_all(connection: &Connection) -> Result<Organizations, String> {
    let mut statement = connection.prepare("SELECT app, session_id, alias, pinned, tags FROM organization").map_err(error)?;
    let mut rows = statement.query([]).map_err(error)?;
    let mut values = Organizations::new();
    while let Some(row) = rows.next().map_err(error)? {
        let app: String = row.get(0).map_err(error)?;
        let app = match app.as_str() { "codex" => AppKind::Codex, "claude" => AppKind::Claude,
            _ => return Err("会话整理客户端无效".into()) };
        let session_id: String = row.get(1).map_err(error)?;
        if !crate::session_manager::parser::valid_session_id(&session_id) { return Err("会话整理 ID 无效".into()); }
        let tags: String = row.get(4).map_err(error)?;
        let value = Organization { alias: row.get(2).map_err(error)?, pinned: row.get(3).map_err(error)?,
            tags: serde_json::from_str(&tags).map_err(|error| format!("会话整理标签无效：{error}"))? };
        validate_value(&value)?;
        values.insert((app, session_id), value);
    }
    Ok(values)
}

pub(super) fn save(connection: &Connection, app: AppKind, id: &str, value: &Organization) -> Result<(), String> {
    if value == &Organization::default() { return remove(connection, app, id); }
    let tags = serde_json::to_string(&value.tags).map_err(|error| format!("无法保存会话标签：{error}"))?;
    connection.execute(
        "INSERT INTO organization(app,session_id,alias,pinned,tags) VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(app,session_id) DO UPDATE SET alias=excluded.alias,pinned=excluded.pinned,tags=excluded.tags",
        params![app.dir_name(), id, value.alias, value.pinned, tags],
    ).map_err(error)?;
    Ok(())
}

pub(super) fn remove(connection: &Connection, app: AppKind, id: &str) -> Result<(), String> {
    connection.execute("DELETE FROM organization WHERE app=?1 AND session_id=?2", params![app.dir_name(), id]).map_err(error)?;
    Ok(())
}

pub(super) fn error(error: rusqlite::Error) -> String {
    format!("会话整理库操作失败：{error}")
}
