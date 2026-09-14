use super::{CodexLedgerFilter, CodexModelPrice, CodexRequestRecord};
use rusqlite::{params, Connection, OpenFlags};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

static SCHEMA_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
const VERSION: i64 = 1;
const SCHEMA: &str = "
CREATE TABLE codex_requests (
 id TEXT PRIMARY KEY NOT NULL, at_ms INTEGER NOT NULL CHECK(at_ms >= 0),
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
PRAGMA user_version = 1;";

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
        let connection = Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(db_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(db_error)?;
        check_version(&connection)?;
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
            (id, at_ms, profile_id, model, status, input_tokens, output_tokens, cache_read_tokens,
             cache_creation_tokens, reasoning_tokens, cost_micros, payload)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) ON CONFLICT(id) DO NOTHING",
                params![
                    record.id,
                    record.at_ms,
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
    /// Price backfill retains the originally accepted provider, model and multiplier.
    pub(crate) fn reprice(
        &self,
        filter: &CodexLedgerFilter,
        prices: &BTreeMap<String, CodexModelPrice>,
    ) -> Result<u64, String> {
        filter.validate()?;
        for price in prices.values() {
            price.validate()?;
        }
        if self.open_read()?.is_none() {
            return Ok(0);
        }
        let mut connection = self.open_write()?;
        let transaction = connection.transaction().map_err(db_error)?;
        let sql = format!("SELECT payload FROM codex_requests {}", super::query::WHERE);
        let rows = {
            let mut statement = transaction.prepare(&sql).map_err(db_error)?;
            let rows = statement
                .query_map(filter.params(), |row| row.get::<_, String>(0))
                .map_err(db_error)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?
        };
        for raw in &rows {
            let mut record: CodexRequestRecord =
                serde_json::from_str(raw).map_err(|_| "Codex 请求记录格式无效；未回填费用")?;
            record.cost = super::estimate(&record, prices)?;
            record.pricing_error = None;
            record.validate()?;
            let payload = serde_json::to_string(&record).map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "UPDATE codex_requests SET cost_micros=?1,payload=?2 WHERE id=?3",
                    params![record.cost_micros()?, payload, record.id],
                )
                .map_err(db_error)?;
        }
        transaction.commit().map_err(db_error)?;
        Ok(rows.len() as u64)
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
    } else if version != VERSION {
        return Err(format!("Codex 账本版本 {version} 不受支持，原数据库未更改"));
    }
    transaction.commit().map_err(db_error)
}
fn check_version(connection: &Connection) -> Result<(), String> {
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(db_error)?;
    if version != VERSION {
        return Err(format!("Codex 账本版本 {version} 不受支持，原数据库未更改"));
    }
    Ok(())
}
pub(super) fn db_error(error: rusqlite::Error) -> String {
    format!("Codex 请求账本数据库错误：{error}")
}
