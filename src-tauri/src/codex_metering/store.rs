use super::CodexRequestRecord;

/// Persistent per-file cursor of the session usage sync pass.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(dead_code)] // reserved: session-usage sync slate
pub(super) struct SessionSyncCursor {
    pub file_path: String,
    pub modified_nanos: i64,
    pub line_offset: i64,
    pub byte_offset: i64,
}

#[derive(Debug, Clone, Copy, Default)]
#[expect(dead_code)] // reserved: session-usage sync slate
pub(super) struct SessionInsertOutcome {
    pub imported: u32,
    pub skipped: u32,
    pub suspected: u32,
}

/// Cross-source dedup window: a session event whose exact token fingerprint
/// matches a successful gateway record within this window was already billed.
#[expect(dead_code)] // reserved: session-usage sync slate
const SESSION_PROXY_DEDUP_WINDOW_MS: i64 = 10 * 60 * 1000;

impl CodexRequestLedger {
    // Whole block belongs to the reserved session-usage sync slate.
    #[expect(dead_code)]
    pub(super) fn load_session_cursors(
        &self,
    ) -> Result<std::collections::BTreeMap<String, SessionSyncCursor>, String> {
        let Some(connection) = self.open_read()? else {
            return Ok(Default::default());
        };
        let mut statement = connection
            .prepare(
                "SELECT file_path,modified_nanos,line_offset,byte_offset FROM codex_session_sync",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    SessionSyncCursor {
                        file_path: String::new(),
                        modified_nanos: row.get(1)?,
                        line_offset: row.get(2)?,
                        byte_offset: row.get(3)?,
                    },
                ))
            })
            .map_err(db_error)?;
        rows.collect::<Result<std::collections::BTreeMap<_, _>, _>>()
            .map_err(db_error)
    }
    /// Inserts session-sourced usage with proxy-fingerprint dedup, then advances
    /// the file cursor in the same transaction so a crash replays the batch.
    pub(super) fn sync_session_entries(
        &self,
        records: &[CodexRequestRecord],
        cursor: &SessionSyncCursor,
    ) -> Result<SessionInsertOutcome, String> {
        for record in records {
            record.validate()?;
        }
        let mut connection = self.open_write()?;
        let transaction = connection.transaction().map_err(db_error)?;
        let mut outcome = SessionInsertOutcome::default();
        for record in records {
            let (Some(input), Some(output), Some(cache_read)) = (
                record.input_tokens,
                record.output_tokens,
                record.cache_read_tokens,
            ) else {
                return Err("Codex 会话记录缺少 token 总量".into());
            };
            if proxy_fingerprint_exists(&transaction, record, input, output, cache_read)?
                || id_exists(&transaction, &record.id)?
            {
                outcome.skipped += 1;
                continue;
            }
            if suspected_session_duplicate(&transaction, record, input, output, cache_read)? {
                outcome.suspected += 1;
            }
            let payload = serde_json::to_string(record).map_err(|error| error.to_string())?;
            let changed = transaction
                .execute(
                    "INSERT INTO codex_requests
                    (id, at_ms, origin, thread_id, profile_id, model, status, input_tokens,
                     output_tokens, cache_read_tokens, cache_creation_tokens, reasoning_tokens,
                     cost_micros, payload)
                    VALUES (?1,?2,'session',?3,NULL,?4,?5,?6,?7,?8,?9,?10,?11,?12)
                    ON CONFLICT(id) DO NOTHING",
                    params![
                        record.id,
                        record.at_ms,
                        record.thread_id,
                        record.mapped_model,
                        record.status,
                        input,
                        output,
                        cache_read,
                        record.cache_creation_tokens,
                        record.reasoning_tokens,
                        record.cost_micros()?,
                        payload
                    ],
                )
                .map_err(db_error)?;
            if changed == 0 {
                outcome.skipped += 1;
            } else {
                outcome.imported += 1;
            }
        }
        upsert_session_cursor(&transaction, cursor)?;
        transaction.commit().map_err(db_error)?;
        Ok(outcome)
    }
    pub(super) fn advance_session_cursor(&self, cursor: &SessionSyncCursor) -> Result<(), String> {
        let mut connection = self.open_write()?;
        let transaction = connection.transaction().map_err(db_error)?;
        upsert_session_cursor(&transaction, cursor)?;
        transaction.commit().map_err(db_error)
    }
    /// Rebuild step: drop session-sourced rows and every cursor, keep proxy rows.
    pub(super) fn clear_session_usage(&self) -> Result<(), String> {
        if self.open_read()?.is_none() {
            return Ok(());
        }
        let mut connection = self.open_write()?;
        let transaction = connection.transaction().map_err(db_error)?;
        transaction
            .execute("DELETE FROM codex_requests WHERE origin='session'", [])
            .map_err(db_error)?;
        transaction
            .execute("DELETE FROM codex_session_sync", [])
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)
    }
    /// Snapshot backup for the rebuild flow; returns None when no ledger exists yet.
    pub(super) fn backup_to(&self, target: &Path) -> Result<bool, String> {
        if !self.path.try_exists().map_err(|error| error.to_string())? {
            return Ok(false);
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("不能创建 Codex 账本备份目录：{error}"))?;
        }
        let connection = Connection::open(&self.path).map_err(db_error)?;
        connection
            .execute("VACUUM INTO ?1", [target.to_string_lossy().as_ref()])
            .map_err(db_error)?;
        Ok(true)
    }
}

#[expect(dead_code)] // reserved: session-usage sync slate
fn upsert_session_cursor(
    transaction: &Connection,
    cursor: &SessionSyncCursor,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO codex_session_sync
             (file_path,modified_nanos,line_offset,byte_offset) VALUES (?1,?2,?3,?4)
             ON CONFLICT(file_path) DO UPDATE SET modified_nanos=?2,line_offset=?3,byte_offset=?4",
            params![
                cursor.file_path,
                cursor.modified_nanos,
                cursor.line_offset,
                cursor.byte_offset
            ],
        )
        .map_err(|error| db_error(error))?;
    Ok(())
}

/// Same-model exact token counts inside the dedup window were already billed by
/// the gateway. Cache creation is unknown in session logs and matches any value.
#[expect(dead_code)] // reserved: session-usage sync slate
fn proxy_fingerprint_exists(
    transaction: &Connection,
    record: &CodexRequestRecord,
    input: u64,
    output: u64,
    cache_read: u64,
) -> Result<bool, String> {
    let model = record.mapped_model.as_deref().unwrap_or("unknown");
    transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM codex_requests
             WHERE origin='proxy' AND status BETWEEN 200 AND 299
               AND input_tokens=?1 AND output_tokens=?2 AND cache_read_tokens=?3
               AND at_ms BETWEEN ?4 AND ?5
               AND (LOWER(model)=LOWER(?6) OR LOWER(model)='unknown' OR LOWER(?6)='unknown'))",
            params![
                input as i64,
                output as i64,
                cache_read as i64,
                record.at_ms as i64 - SESSION_PROXY_DEDUP_WINDOW_MS,
                record.at_ms as i64 + SESSION_PROXY_DEDUP_WINDOW_MS,
                model
            ],
            |row| row.get(0),
        )
        .map_err(db_error)
}

#[expect(dead_code)] // reserved: session-usage sync slate
fn id_exists(transaction: &Connection, id: &str) -> Result<bool, String> {
    transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM codex_requests WHERE id=?1)",
            [id],
            |row| row.get(0),
        )
        .map_err(db_error)
}

#[expect(dead_code)] // reserved: session-usage sync slate
fn suspected_session_duplicate(
    transaction: &Connection,
    record: &CodexRequestRecord,
    input: u64,
    output: u64,
    cache_read: u64,
) -> Result<bool, String> {
    let model = record.mapped_model.as_deref().unwrap_or("unknown");
    transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM codex_requests
             WHERE origin='session' AND id<>?1 AND LOWER(model)=LOWER(?2)
               AND input_tokens=?3 AND output_tokens=?4 AND cache_read_tokens=?5
               AND at_ms BETWEEN ?6 AND ?7)",
            params![
                record.id,
                model,
                input as i64,
                output as i64,
                cache_read as i64,
                record.at_ms as i64 - SESSION_PROXY_DEDUP_WINDOW_MS,
                record.at_ms as i64 + SESSION_PROXY_DEDUP_WINDOW_MS
            ],
            |row| row.get(0),
        )
        .map_err(db_error)
}

use rusqlite::{params, Connection};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

static SCHEMA_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
const VERSION: i64 = 2;
const SCHEMA: &str = "
CREATE TABLE codex_requests (
 id TEXT PRIMARY KEY NOT NULL, at_ms INTEGER NOT NULL CHECK(at_ms >= 0),
 origin TEXT NOT NULL DEFAULT 'proxy' CHECK(origin IN ('proxy','session')), thread_id TEXT,
 profile_id TEXT, model TEXT, status INTEGER CHECK(status BETWEEN 100 AND 599),
 input_tokens INTEGER CHECK(input_tokens >= 0), output_tokens INTEGER CHECK(output_tokens >= 0),
 cache_read_tokens INTEGER CHECK(cache_read_tokens >= 0),
 cache_creation_tokens INTEGER CHECK(cache_creation_tokens >= 0),
 reasoning_tokens INTEGER CHECK(reasoning_tokens >= 0), cost_micros INTEGER CHECK(cost_micros >= 0),
 payload TEXT NOT NULL CHECK(json_valid(payload))
);
CREATE INDEX codex_requests_time ON codex_requests(at_ms DESC, id);
CREATE INDEX codex_requests_provider_time ON codex_requests(profile_id, at_ms DESC);
CREATE INDEX codex_requests_model_time ON codex_requests(model, at_ms DESC);
CREATE TABLE codex_session_sync (
 file_path TEXT PRIMARY KEY NOT NULL, modified_nanos INTEGER NOT NULL,
 line_offset INTEGER NOT NULL, byte_offset INTEGER NOT NULL
);
PRAGMA user_version = 2;";

/// v1 → v2: `origin`/`thread_id` columns plus the session sync cursor table.
/// Existing payloads are rewritten once so the strict record contract keeps
/// rejecting rows that predate the origin discriminator.
fn migrate_v1_to_v2(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "ALTER TABLE codex_requests ADD COLUMN origin TEXT NOT NULL DEFAULT 'proxy';
             ALTER TABLE codex_requests ADD COLUMN thread_id TEXT;
             CREATE TABLE codex_session_sync (
              file_path TEXT PRIMARY KEY NOT NULL, modified_nanos INTEGER NOT NULL,
              line_offset INTEGER NOT NULL, byte_offset INTEGER NOT NULL
             );",
        )
        .map_err(db_error)?;
    let rows: Vec<(String, String)> = {
        let mut statement = connection
            .prepare("SELECT id,payload FROM codex_requests")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(db_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?
    };
    for (id, payload) in rows {
        let mut value: serde_json::Value = serde_json::from_str(&payload)
            .map_err(|_| "Codex 账本 v1 载荷无效，未迁移".to_string())?;
        let Some(object) = value.as_object_mut() else {
            return Err("Codex 账本 v1 载荷无效，未迁移".into());
        };
        if object.contains_key("origin") {
            return Err("Codex 账本 v1 载荷已含来源字段，未迁移".into());
        }
        object.insert("origin".into(), "proxy".into());
        connection
            .execute(
                "UPDATE codex_requests SET payload=?1 WHERE id=?2",
                params![value.to_string(), id],
            )
            .map_err(db_error)?;
    }
    connection
        .pragma_update(None, "user_version", VERSION)
        .map_err(db_error)
}

#[derive(Clone)]
pub(crate) struct CodexRequestLedger {
    pub(super) path: PathBuf,
}
impl CodexRequestLedger {
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            path: root.join("codex/requests.sqlite3"),
        }
    }
    fn open_write(&self) -> Result<Connection, String> {
        let _guard = SCHEMA_GATE.lock().map_err(|_| "Codex 账本初始化锁不可用")?;
        std::fs::create_dir_all(self.path.parent().ok_or("Codex 账本路径无效")?)
            .map_err(|error| format!("不能创建 Codex 账本目录：{error}"))?;
        let mut connection = Connection::open(&self.path).map_err(db_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(db_error)?;
        initialize(&mut connection)?;
        Ok(connection)
    }
    pub(super) fn open_read(&self) -> Result<Option<Connection>, String> {
        let _guard = SCHEMA_GATE.lock().map_err(|_| "Codex 账本初始化锁不可用")?;
        if !self.path.try_exists().map_err(|error| error.to_string())? {
            return Ok(None);
        }
        // Opened read-write so a v1 ledger still migrates on the first read;
        // the transaction gate keeps concurrent readers serialized.
        let mut connection = Connection::open(&self.path).map_err(db_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(db_error)?;
        initialize(&mut connection)?;
        Ok(Some(connection))
    }
    pub(crate) fn append(&self, record: &CodexRequestRecord) -> Result<(), String> {
        record.validate()?;
        let payload = serde_json::to_string(record).map_err(|error| error.to_string())?;
        let mut connection = self.open_write()?;
        let transaction = connection.transaction().map_err(db_error)?;
        let changed = transaction
            .execute(
                "INSERT INTO codex_requests
            (id, at_ms, origin, thread_id, profile_id, model, status, input_tokens, output_tokens,
             cache_read_tokens, cache_creation_tokens, reasoning_tokens, cost_micros, payload)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14) ON CONFLICT(id) DO NOTHING",
                params![
                    record.id,
                    record.at_ms,
                    record.origin.as_str(),
                    record.thread_id,
                    record.profile_id,
                    record.mapped_model,
                    record.status,
                    record.input_tokens,
                    record.output_tokens,
                    record.cache_read_tokens,
                    record.cache_creation_tokens,
                    record.reasoning_tokens,
                    record.cost_micros()?,
                    payload
                ],
            )
            .map_err(db_error)?;
        if changed == 0 {
            let existing: String = transaction
                .query_row(
                    "SELECT payload FROM codex_requests WHERE id=?1",
                    [&record.id],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            if existing != payload {
                return Err("Codex 请求标识已存在且内容不同，未覆盖原记录".into());
            }
        }
        transaction.commit().map_err(db_error)
    }
}
fn initialize(connection: &mut Connection) -> Result<(), String> {
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(db_error)?;
    let version: i64 = transaction
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(db_error)?;
    if version == 0 {
        let tables: i64 = transaction.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [], |row| row.get(0)).map_err(db_error)?;
        if tables != 0 {
            return Err("Codex 账本 schema 未知，未重建或覆盖数据库".into());
        }
        transaction.execute_batch(SCHEMA).map_err(db_error)?;
    } else if version == 1 {
        migrate_v1_to_v2(&transaction)?;
    } else if version != VERSION {
        return Err(format!("Codex 账本版本 {version} 不受支持，原数据库未更改"));
    }
    transaction.commit().map_err(db_error)
}
pub(super) fn db_error(error: rusqlite::Error) -> String {
    format!("Codex 请求账本数据库错误：{error}")
}
