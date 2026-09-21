//! The persistent probe history: one SQLite database owning every batch and
//! run record. It is the only durable source of probe state — the in-memory
//! registry keeps control handles, and every read (including "the current
//! batch") is served from here.

use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::time::Duration;

static SCHEMA_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
const VERSION: i64 = 1;
const SCHEMA: &str = "
CREATE TABLE probe_batches (
 id TEXT PRIMARY KEY NOT NULL,
 started_at TEXT NOT NULL,
 finished_at TEXT,
 status TEXT NOT NULL CHECK(status IN ('running','completed','cancelled','failed','interrupted','config-changed')),
 planned_runs INTEGER NOT NULL CHECK(planned_runs BETWEEN 1 AND 10),
 question_id TEXT NOT NULL,
 question_label TEXT NOT NULL,
 question_text TEXT NOT NULL,
 expected_answer TEXT NOT NULL,
 grading_version TEXT NOT NULL,
 cli_version TEXT,
 profile_id TEXT,
 profile_name TEXT,
 profile_model TEXT,
 reasoning_effort TEXT,
 connection_identity TEXT,
 config_fingerprint TEXT NOT NULL,
 status_error TEXT
);
CREATE INDEX probe_batches_time ON probe_batches(started_at DESC);
CREATE TABLE probe_runs (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 batch_id TEXT NOT NULL,
 seq INTEGER NOT NULL CHECK(seq >= 1),
 status TEXT NOT NULL CHECK(status IN ('running','passed','failed','undetermined')),
 session_id TEXT,
 final_answer TEXT,
 reported_model TEXT,
 duration_ms INTEGER CHECK(duration_ms >= 0),
 reasoning_tokens INTEGER CHECK(reasoning_tokens >= 0),
 total_tokens INTEGER CHECK(total_tokens >= 0),
 execution_error TEXT,
 usage_error TEXT,
 UNIQUE(batch_id, seq)
);
CREATE INDEX probe_runs_batch ON probe_runs(batch_id, seq);
PRAGMA user_version = 1;";

pub(crate) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

mod types;
pub(crate) use types::*;
mod history;
mod recovery;
mod writes;

#[derive(Clone)]
pub(crate) struct ProbeLedger {
    path: PathBuf,
}

const BATCH_COLUMNS: &str = "id, started_at, finished_at, status, planned_runs, question_id, \
     question_label, question_text, expected_answer, grading_version, cli_version, \
     profile_id, profile_name, profile_model, reasoning_effort, connection_identity, \
     config_fingerprint, status_error";

fn map_batch(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProbeBatchRecord> {
    Ok(ProbeBatchRecord {
        id: row.get(0)?,
        started_at: row.get(1)?,
        finished_at: row.get(2)?,
        status: ProbeBatchStatus::parse(&row.get::<_, String>(3)?).unwrap_or(ProbeBatchStatus::Failed),
        planned_runs: row.get::<_, i64>(4)?.max(0) as u32,
        question: ProbeQuestionSnapshot {
            id: row.get(5)?,
            label: row.get(6)?,
            text: row.get(7)?,
            expected_answer: row.get(8)?,
        },
        grading_version: row.get(9)?,
        cli_version: row.get(10)?,
        config: ProbeConfigRecord {
            profile_id: row.get(11)?,
            profile_name: row.get(12)?,
            profile_model: row.get(13)?,
            reasoning_effort: row.get(14)?,
            connection_identity: row.get(15)?,
            fingerprint: row.get(16)?,
        },
        status_error: row.get(17)?,
    })
}

fn map_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProbeRunRecord> {
    Ok(ProbeRunRecord {
        seq: row.get::<_, i64>(0)?.max(0) as u32,
        status: ProbeRunStatus::parse(&row.get::<_, String>(1)?).unwrap_or(ProbeRunStatus::Undetermined),
        session_id: row.get(2)?,
        final_answer: row.get(3)?,
        reported_model: row.get(4)?,
        duration_ms: row.get::<_, Option<i64>>(5)?.map(|value| value.max(0) as u64),
        reasoning_tokens: row.get::<_, Option<i64>>(6)?.map(|value| value.max(0) as u64),
        total_tokens: row.get::<_, Option<i64>>(7)?.map(|value| value.max(0) as u64),
        execution_error: row.get(8)?,
        usage_error: row.get(9)?,
    })
}

impl ProbeLedger {
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            path: root.join("codex/probes.sqlite3"),
        }
    }

    fn open_write(&self) -> Result<Connection, String> {
        let _guard = SCHEMA_GATE.lock().map_err(|_| "检测历史初始化锁不可用")?;
        std::fs::create_dir_all(self.path.parent().ok_or("检测历史路径无效")?)
            .map_err(|error| format!("不能创建检测历史目录：{error}"))?;
        let mut connection = Connection::open(&self.path).map_err(db_error)?;
        connection.busy_timeout(Duration::from_secs(5)).map_err(db_error)?;
        initialize(&mut connection)?;
        Ok(connection)
    }

    /// Returns `None` when no history database exists yet.
    fn open_read(&self) -> Result<Option<Connection>, String> {
        let _guard = SCHEMA_GATE.lock().map_err(|_| "检测历史初始化锁不可用")?;
        if !self.path.try_exists().map_err(|error| error.to_string())? {
            return Ok(None);
        }
        let mut connection = Connection::open(&self.path).map_err(db_error)?;
        connection.busy_timeout(Duration::from_secs(5)).map_err(db_error)?;
        initialize(&mut connection)?;
        Ok(Some(connection))
    }

    // -------------------------------------------------------------- reads

    pub(crate) fn load_batch(&self, batch_id: &str) -> Result<Option<ProbeBatchRecord>, String> {
        let Some(connection) = self.open_read()? else {
            return Ok(None);
        };
        connection
            .query_row(
                &format!("SELECT {BATCH_COLUMNS} FROM probe_batches WHERE id=?1"),
                [batch_id],
                map_batch,
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(db_error(other)),
            })
    }

    /// The newest batch — live or already finished; this is what the radar
    /// panel reconnects to after a refresh or restart.
    pub(crate) fn current_batch(&self) -> Result<Option<ProbeBatchRecord>, String> {
        let Some(connection) = self.open_read()? else {
            return Ok(None);
        };
        connection
            .query_row(
                &format!(
                    "SELECT {BATCH_COLUMNS} FROM probe_batches \
                     ORDER BY started_at DESC, rowid DESC LIMIT 1"
                ),
                [],
                map_batch,
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(db_error(other)),
            })
    }

    pub(crate) fn load_runs(&self, batch_id: &str) -> Result<Vec<ProbeRunRecord>, String> {
        let Some(connection) = self.open_read()? else {
            return Ok(Vec::new());
        };
        let mut statement = connection
            .prepare(
                "SELECT seq, status, session_id, final_answer, reported_model, duration_ms, \
                 reasoning_tokens, total_tokens, execution_error, usage_error \
                 FROM probe_runs WHERE batch_id=?1 ORDER BY seq",
            )
            .map_err(db_error)?;
        let rows = statement.query_map([batch_id], map_run).map_err(db_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
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
        let tables: i64 = transaction
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        if tables != 0 {
            return Err("检测历史 schema 未知，未重建或覆盖数据库".to_string());
        }
        transaction.execute_batch(SCHEMA).map_err(db_error)?;
    } else if version != VERSION {
        return Err(format!("检测历史版本 {version} 不受支持，原数据库未更改"));
    }
    transaction.commit().map_err(db_error)
}

fn db_error(error: rusqlite::Error) -> String {
    format!("检测历史数据库错误：{error}")
}

#[cfg(test)]
mod tests;
