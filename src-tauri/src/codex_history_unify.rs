//! Codex 跨供应商统一历史迁移（S03）。
//!
//! ASB 的 Codex 投影恒为内建 `openai` provider，因此统一桶就是 `openai`
//! 本身：本模块把遗留来源桶（CC 预设 id、旧工具写入的自定义 id 等）的存量
//! 会话迁入 `openai` 桶——JSONL 的 `session_meta` 行与 state DB 的
//! `threads.model_provider` 行。迁移前逐对象备份到账本目录；失败不写完成
//! 标记，下次可重跑（幂等）。还原按备份账本的对象 id 精确翻回原桶，绝不
//! 触碰账本之外（迁移之后新增）的会话；还原动作自身也先备份。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{backup::Backup, params_from_iter, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) const TARGET_PROVIDER_ID: &str = "openai";
pub(crate) const MIGRATION_NAME: &str = "codex-history-unify-v1";
const RESTORE_BACKUP_NAME: &str = "codex-history-unify-restore-v1";
/// SQLite 变量上限保守值，IN 列表按此分块。
const STATE_DB_ID_CHUNK: usize = 500;
const UNRECOGNIZED_STATE_DB: &str = "Codex state DB 无法访问";

/// 已知的历史遗留来源桶 id（the source application 固定基线的预设 id 列表）；扫描发现的
/// 其他自定义桶只能由用户显式选择后迁移。
pub(crate) const LEGACY_SOURCE_PROVIDER_IDS: &[&str] = &[
    "ccswitch",
    "aicodemirror",
    "aicoding",
    "aigocode",
    "aihubmix",
    "ark_agentplan",
    "bailian",
    "bailing",
    "byteplus",
    "claudecn",
    "compshare",
    "compshare_coding",
    "crazyrouter",
    "ctok",
    "cubence",
    "deepseek",
    "dmxapi",
    "doubaoseed",
    "eflowcode",
    "etok",
    "kimi",
    "lemondata",
    "longcat",
    "micu",
    "minimax",
    "minimax_en",
    "modelscope",
    "novita",
    "nvidia",
    "openrouter",
    "packycode",
    "patewayai",
    "pipellm",
    "qianfan_coding",
    "relaxycode",
    "rightcode",
    "runapi",
    "shengsuanyun",
    "siliconflow",
    "siliconflow_en",
    "sssaicode",
    "stepfun",
    "stepfun_en",
    "therouter",
    "xiaomi_mimo",
    "xiaomi_mimo_token_plan",
    "zhipu_glm",
    "zhipu_glm_en",
];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BucketScan {
    pub(crate) provider_id: String,
    pub(crate) jsonl_files: u64,
    pub(crate) state_rows: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnifyOutcome {
    pub(crate) source_provider_ids: Vec<String>,
    pub(crate) migrated_jsonl_files: usize,
    pub(crate) migrated_state_rows: usize,
    pub(crate) skipped_reason: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RestoreOutcome {
    pub(crate) restored_jsonl_files: usize,
    pub(crate) restored_state_rows: usize,
    pub(crate) skipped_reason: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UnifyMarker {
    completed_at: String,
    target_provider_id: String,
    source_provider_ids: Vec<String>,
    migrated_jsonl_files: usize,
    migrated_state_rows: usize,
    codex_dir_key: String,
}

fn marker_path(root: &Path) -> PathBuf {
    root.join("codex-history-unify.json")
}

fn backup_root(root: &Path) -> PathBuf {
    root.join("backups").join(MIGRATION_NAME)
}

fn restore_backup_root(root: &Path) -> PathBuf {
    root.join("backups").join(RESTORE_BACKUP_NAME)
}

fn codex_dir_key(codex_root: &Path) -> String {
    codex_root
        .canonicalize()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| codex_root.to_string_lossy().into_owned())
}

fn read_marker(root: &Path) -> Option<UnifyMarker> {
    let raw = fs::read_to_string(marker_path(root)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_marker(root: &Path, marker: &UnifyMarker) -> Result<(), String> {
    let text = serde_json::to_string_pretty(marker).map_err(|_| "迁移标记无法序列化")?;
    let path = marker_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "无法创建迁移标记目录".to_string())?;
    }
    atomic_write(&path, text.as_bytes())
}

fn clear_marker(root: &Path) -> bool {
    fs::remove_file(marker_path(root)).is_ok()
}

fn session_roots(codex_root: &Path) -> [PathBuf; 2] {
    [
        codex_root.join("sessions"),
        codex_root.join("archived_sessions"),
    ]
}

fn collect_jsonl_files(dir: &Path, files: &mut Vec<PathBuf>, depth: u8, max_depth: u8) {
    if depth > max_depth || !dir.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, files, depth + 1, max_depth);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            files.push(path);
        }
    }
}

fn collect_sqlite_files(dir: &Path, files: &mut Vec<PathBuf>, depth: u8, max_depth: u8) {
    if depth > max_depth || !dir.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sqlite_files(&path, files, depth + 1, max_depth);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("sqlite") {
            files.push(path);
        }
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("写入路径无效".to_string())?;
    let temporary = parent.join(format!(
        "{}.{}.tmp",
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        fs::write(&temporary, bytes).map_err(|_| "无法写入临时文件".to_string())?;
        fs::rename(&temporary, path).map_err(|_| "无法原子替换目标文件".to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn unchanged_since(
    path: &Path,
    modified: Option<std::time::SystemTime>,
    len: u64,
) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|_| "会话文件在迁移中消失".to_string())?;
    if metadata.modified().ok() != modified || metadata.len() != len {
        return Err("会话文件在迁移期间被修改，已放弃本次改写".into());
    }
    Ok(())
}

/// One `session_meta` line's routing facts: the session id and its provider.
fn session_meta_facts(line: &str) -> Option<(String, String)> {
    if !line.contains("\"session_meta\"") || !line.contains("\"model_provider\"") {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    let payload = value.get("payload")?;
    let id = payload.get("id")?.as_str()?.to_string();
    let provider = payload.get("model_provider")?.as_str()?.to_string();
    Some((id, provider))
}

/// 只读发现：按来源桶统计存量会话（JSONL 文件数与 state DB 行数）。
pub(crate) fn scan_buckets(
    codex_root: &Path,
    config_text: &str,
) -> Result<Vec<BucketScan>, String> {
    let mut jsonl_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut files = Vec::new();
    for session_root in session_roots(codex_root) {
        collect_jsonl_files(&session_root, &mut files, 0, 8);
    }
    for file in &files {
        if let Some((_, provider)) = file_session_meta(file) {
            *jsonl_counts.entry(provider).or_insert(0) += 1;
        }
    }
    let mut state_counts: BTreeMap<String, u64> = BTreeMap::new();
    for db_path in
        crate::session_manager::codex_titles::state_db_paths(codex_root, config_text, None)
    {
        collect_state_bucket_counts(&db_path, &mut state_counts)?;
    }
    let mut provider_ids: BTreeSet<String> = jsonl_counts.keys().cloned().collect();
    provider_ids.extend(state_counts.keys().cloned());
    Ok(provider_ids
        .into_iter()
        .map(|provider_id| BucketScan {
            jsonl_files: jsonl_counts.get(&provider_id).copied().unwrap_or(0),
            state_rows: state_counts.get(&provider_id).copied().unwrap_or(0),
            provider_id,
        })
        .collect())
}

fn file_session_meta(file: &Path) -> Option<(String, String)> {
    let content = fs::read_to_string(file).ok()?;
    for line in content.lines() {
        if let Some(facts) = session_meta_facts(line) {
            return Some(facts);
        }
    }
    None
}

fn collect_state_bucket_counts(
    db_path: &Path,
    counts: &mut BTreeMap<String, u64>,
) -> Result<(), String> {
    if !db_path.exists() {
        return Ok(());
    }
    let conn = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| UNRECOGNIZED_STATE_DB.to_string())?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|_| UNRECOGNIZED_STATE_DB.to_string())?;
    if !state_db_ready(&conn) {
        return Ok(());
    }
    let mut statement = conn
        .prepare("SELECT model_provider, COUNT(*) FROM threads GROUP BY model_provider")
        .map_err(|_| UNRECOGNIZED_STATE_DB.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|_| UNRECOGNIZED_STATE_DB.to_string())?;
    for row in rows.flatten() {
        *counts.entry(row.0).or_insert(0) += row.1.max(0) as u64;
    }
    Ok(())
}

fn state_db_ready(conn: &Connection) -> bool {
    table_exists(conn, "threads") && has_column(conn, "threads", "model_provider")
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .unwrap_or(false)
}

fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
    let Ok(columns) = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get::<_, String>(1))
                .map(|rows| rows.flatten().collect::<Vec<String>>())
        })
    else {
        return false;
    };
    columns.into_iter().any(|name| name == column)
}

fn open_live_state_db(db_path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(db_path).map_err(|_| UNRECOGNIZED_STATE_DB.to_string())?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|_| UNRECOGNIZED_STATE_DB.to_string())?;
    Ok(conn)
}

fn atomic_write_and_backup(
    root: &Path,
    path: &Path,
    codex_root: &Path,
    subdirectory: &str,
    rewritten: &[u8],
    modified_before: Option<std::time::SystemTime>,
    len_before: u64,
) -> Result<(), String> {
    let relative = path
        .strip_prefix(codex_root)
        .map_err(|_| "对象位于 Codex 目录之外".to_string())?;
    unchanged_since(path, modified_before, len_before)?;
    let backup = backup_root(root).join(subdirectory).join(relative);
    if let Some(parent) = backup.parent() {
        fs::create_dir_all(parent).map_err(|_| "无法创建备份目录".to_string())?;
    }
    fs::copy(path, &backup).map_err(|_| "无法备份原对象".to_string())?;
    unchanged_since(path, modified_before, len_before)?;
    atomic_write(path, rewritten)
}

/// Rewrites every `session_meta` line whose (current provider, session id)
/// pair is accepted by `remap`, with before/after change checks and a ledger
/// backup of the original file.
fn rewrite_jsonl_files(
    root: &Path,
    codex_root: &Path,
    remap: &dyn Fn(&str, &str) -> Option<String>,
) -> Result<usize, String> {
    let mut files = Vec::new();
    for session_root in session_roots(codex_root) {
        collect_jsonl_files(&session_root, &mut files, 0, 8);
    }
    let mut migrated = 0;
    for path in files {
        let metadata = fs::metadata(&path).ok();
        let modified_before = metadata.as_ref().and_then(|meta| meta.modified().ok());
        let len_before = metadata.as_ref().map(|meta| meta.len()).unwrap_or(0);
        let content = fs::read_to_string(&path).map_err(|_| "无法读取会话文件".to_string())?;
        let mut rewritten = String::with_capacity(content.len());
        let mut changed = false;
        for segment in content.split_inclusive('\n') {
            let (line, newline) = match segment.strip_suffix('\n') {
                Some(line) => (line, "\n"),
                None => (segment, ""),
            };
            let next = if line.contains("\"session_meta\"") && line.contains("\"model_provider\"") {
                serde_json::from_str::<Value>(line)
                    .ok()
                    .filter(|value| {
                        value.get("type").and_then(Value::as_str) == Some("session_meta")
                    })
                    .and_then(|mut value| {
                        let payload = value.get_mut("payload")?.as_object_mut()?;
                        let current = payload.get("model_provider")?.as_str()?.to_string();
                        let session_id = payload
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let next_provider = remap(&current, session_id)?;
                        payload.insert("model_provider".into(), Value::String(next_provider));
                        serde_json::to_string(&value).ok()
                    })
            } else {
                None
            };
            match next {
                Some(next) => {
                    rewritten.push_str(&next);
                    changed = true;
                }
                None => rewritten.push_str(line),
            }
            rewritten.push_str(newline);
        }
        if !changed {
            continue;
        }
        atomic_write_and_backup(
            root,
            &path,
            codex_root,
            "jsonl",
            rewritten.as_bytes(),
            modified_before,
            len_before,
        )?;
        migrated += 1;
    }
    Ok(migrated)
}

fn backup_state_db(
    root: &Path,
    db_path: &Path,
    codex_root: &Path,
    source: &Connection,
    restore: bool,
) -> Result<(), String> {
    let relative = db_path
        .strip_prefix(codex_root)
        .map_err(|_| "state DB 位于 Codex 目录之外".to_string())?;
    let base = if restore {
        restore_backup_root(root)
    } else {
        backup_root(root)
    };
    let backup_path = base.join("state").join(relative);
    if let Some(parent) = backup_path.parent() {
        fs::create_dir_all(parent).map_err(|_| "无法创建 state DB 备份目录".to_string())?;
    }
    let mut target =
        Connection::open(&backup_path).map_err(|_| "无法创建 state DB 备份".to_string())?;
    let backup =
        Backup::new(source, &mut target).map_err(|_| "无法初始化 state DB 备份".to_string())?;
    backup
        .run_to_completion(5, Duration::from_millis(25), None)
        .map_err(|_| "无法写入 state DB 备份".to_string())
}

/// 把来源桶的存量会话迁入统一桶（`openai`）。幂等：绑定 Codex 目录的完成
/// 标记存在时跳过；任何失败都不写标记。
pub(crate) fn migrate_to_unified(
    root: &Path,
    codex_root: &Path,
    config_text: &str,
    source_provider_ids: &BTreeSet<String>,
) -> Result<UnifyOutcome, String> {
    let dir_key = codex_dir_key(codex_root);
    if let Some(marker) = read_marker(root) {
        if marker.codex_dir_key == dir_key {
            return Ok(UnifyOutcome {
                skipped_reason: Some("already_migrated"),
                source_provider_ids: marker.source_provider_ids,
                migrated_jsonl_files: marker.migrated_jsonl_files,
                migrated_state_rows: marker.migrated_state_rows,
            });
        }
    }
    let sources: BTreeSet<String> = source_provider_ids
        .iter()
        .filter(|id| id.as_str() != TARGET_PROVIDER_ID && !id.trim().is_empty())
        .cloned()
        .collect();
    if sources.is_empty() {
        return Ok(UnifyOutcome {
            source_provider_ids: Vec::new(),
            migrated_jsonl_files: 0,
            migrated_state_rows: 0,
            skipped_reason: Some("no_source_provider_ids"),
        });
    }

    let migrated_jsonl_files = rewrite_jsonl_files(root, codex_root, &|current, _| {
        sources
            .contains(current)
            .then(|| TARGET_PROVIDER_ID.to_string())
    })?;

    let mut migrated_state_rows = 0;
    for db_path in
        crate::session_manager::codex_titles::state_db_paths(codex_root, config_text, None)
    {
        migrated_state_rows += migrate_state_db(root, codex_root, &db_path, &sources)?;
    }

    write_marker(
        root,
        &UnifyMarker {
            completed_at: chrono::Utc::now().to_rfc3339(),
            target_provider_id: TARGET_PROVIDER_ID.into(),
            source_provider_ids: sources.into_iter().collect(),
            migrated_jsonl_files,
            migrated_state_rows,
            codex_dir_key: dir_key,
        },
    )?;
    Ok(UnifyOutcome {
        source_provider_ids: Vec::new(),
        migrated_jsonl_files,
        migrated_state_rows,
        skipped_reason: None,
    })
}

fn migrate_state_db(
    root: &Path,
    codex_root: &Path,
    db_path: &Path,
    sources: &BTreeSet<String>,
) -> Result<usize, String> {
    if !db_path.exists() {
        return Ok(0);
    }
    let mut conn = open_live_state_db(db_path)?;
    if !state_db_ready(&conn) {
        return Ok(0);
    }
    let placeholders = std::iter::repeat("?")
        .take(sources.len())
        .collect::<Vec<_>>()
        .join(", ");
    let matching: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM threads WHERE model_provider IN ({placeholders})"),
            params_from_iter(sources.iter()),
            |row| row.get(0),
        )
        .map_err(|_| UNRECOGNIZED_STATE_DB.to_string())?;
    if matching == 0 {
        return Ok(0);
    }
    backup_state_db(root, db_path, codex_root, &conn, false)?;

    let tx = conn
        .transaction()
        .map_err(|_| UNRECOGNIZED_STATE_DB.to_string())?;
    let ids: Vec<&String> = sources.iter().collect();
    let mut changed = 0;
    for chunk in ids.chunks(STATE_DB_ID_CHUNK) {
        let placeholders = std::iter::repeat("?")
            .take(chunk.len())
            .collect::<Vec<_>>()
            .join(", ");
        let mut values: Vec<&dyn rusqlite::ToSql> = vec![&TARGET_PROVIDER_ID];
        values.extend(chunk.iter().map(|id| id as &dyn rusqlite::ToSql));
        changed += tx
            .execute(
                &format!(
                    "UPDATE threads SET model_provider = ? WHERE model_provider IN ({placeholders})"
                ),
                values.as_slice(),
            )
            .map_err(|_| UNRECOGNIZED_STATE_DB.to_string())?;
    }
    tx.commit().map_err(|_| UNRECOGNIZED_STATE_DB.to_string())?;
    Ok(changed)
}

/// Whether the directory-bound completion marker is present.
pub(crate) fn migration_completed(root: &Path, codex_root: &Path) -> bool {
    read_marker(root)
        .map(|marker| marker.codex_dir_key == codex_dir_key(codex_root))
        .unwrap_or(false)
}

pub(crate) fn has_backup(root: &Path) -> bool {
    let ledger = backup_root(root);
    let mut files = Vec::new();
    collect_jsonl_files(&ledger.join("jsonl"), &mut files, 0, 10);
    if !files.is_empty() {
        return true;
    }
    let mut dbs = Vec::new();
    collect_sqlite_files(&ledger.join("state"), &mut dbs, 0, 4);
    !dbs.is_empty()
}

/// 按备份账本把迁移对象精确翻回原桶：只有账本内（且当前已在统一桶）的对象
/// 会被改写，迁移之后新增的会话不受影响。还原动作自身也先备份到还原账本。
pub(crate) fn restore_from_backups(
    root: &Path,
    codex_root: &Path,
    config_text: &str,
) -> Result<RestoreOutcome, String> {
    let ledger = backup_root(root);
    let mut session_sources: BTreeMap<String, String> = BTreeMap::new();
    let mut ledger_files = Vec::new();
    collect_jsonl_files(&ledger.join("jsonl"), &mut ledger_files, 0, 10);
    for file in &ledger_files {
        if let Some((id, provider)) = file_session_meta(file) {
            if provider != TARGET_PROVIDER_ID {
                session_sources.insert(id, provider);
            }
        }
    }
    let mut thread_sources: BTreeMap<String, String> = BTreeMap::new();
    let mut ledger_dbs = Vec::new();
    collect_sqlite_files(&ledger.join("state"), &mut ledger_dbs, 0, 4);
    for db in &ledger_dbs {
        collect_thread_sources_from_backup(db, &mut thread_sources);
    }
    if session_sources.is_empty() && thread_sources.is_empty() {
        return Ok(RestoreOutcome {
            restored_jsonl_files: 0,
            restored_state_rows: 0,
            skipped_reason: Some("no_backup_ledger"),
        });
    }

    let restored_jsonl_files = rewrite_jsonl_files(root, codex_root, &|current, session_id| {
        if current != TARGET_PROVIDER_ID {
            return None;
        }
        session_sources.get(session_id).cloned()
    })?;

    let mut restored_state_rows = 0;
    for db_path in
        crate::session_manager::codex_titles::state_db_paths(codex_root, config_text, None)
    {
        restored_state_rows += restore_state_db(root, codex_root, &db_path, &thread_sources)?;
    }
    let mut outcome = RestoreOutcome {
        restored_jsonl_files,
        restored_state_rows,
        skipped_reason: None,
    };
    if restored_jsonl_files == 0 && restored_state_rows == 0 {
        outcome.skipped_reason = Some("nothing_to_restore");
    } else {
        clear_marker(root);
    }
    Ok(outcome)
}

fn collect_thread_sources_from_backup(db_path: &Path, sources: &mut BTreeMap<String, String>) {
    let Ok(conn) = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return;
    };
    if !state_db_ready(&conn) {
        return;
    }
    let Ok(mut statement) =
        conn.prepare("SELECT id, model_provider FROM threads WHERE model_provider != ?1")
    else {
        return;
    };
    let rows = statement.query_map([TARGET_PROVIDER_ID], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    });
    if let Ok(rows) = rows {
        for row in rows.flatten() {
            sources.insert(row.0, row.1);
        }
    }
}

fn restore_state_db(
    root: &Path,
    codex_root: &Path,
    db_path: &Path,
    thread_sources: &BTreeMap<String, String>,
) -> Result<usize, String> {
    if !db_path.exists() || thread_sources.is_empty() {
        return Ok(0);
    }
    let mut conn = open_live_state_db(db_path)?;
    if !state_db_ready(&conn) {
        return Ok(0);
    }
    let mut matching: i64 = 0;
    let ids: Vec<&String> = thread_sources.keys().collect();
    for chunk in ids.chunks(STATE_DB_ID_CHUNK) {
        let placeholders = std::iter::repeat("?")
            .take(chunk.len())
            .collect::<Vec<_>>()
            .join(", ");
        let count_sql = format!(
            "SELECT COUNT(*) FROM threads WHERE model_provider = ? AND id IN ({placeholders})"
        );
        let mut values: Vec<&dyn rusqlite::ToSql> = vec![&TARGET_PROVIDER_ID];
        values.extend(chunk.iter().map(|id| id as &dyn rusqlite::ToSql));
        matching += conn
            .query_row(&count_sql, values.as_slice(), |row| row.get(0))
            .unwrap_or(0);
    }
    if matching == 0 {
        return Ok(0);
    }
    backup_state_db(root, db_path, codex_root, &conn, true)?;

    let tx = conn
        .transaction()
        .map_err(|_| UNRECOGNIZED_STATE_DB.to_string())?;
    let mut changed = 0;
    for chunk in ids.chunks(STATE_DB_ID_CHUNK) {
        let placeholders = std::iter::repeat("?")
            .take(chunk.len())
            .collect::<Vec<_>>()
            .join(", ");
        let update_sql = format!(
            "UPDATE threads SET model_provider = ? WHERE model_provider = ? AND id IN ({placeholders})"
        );
        for (id, provider) in thread_sources {
            if !chunk.contains(&id) {
                continue;
            }
            changed += tx
                .execute(
                    &update_sql,
                    rusqlite::params![provider, TARGET_PROVIDER_ID, id],
                )
                .map_err(|_| UNRECOGNIZED_STATE_DB.to_string())?;
        }
    }
    tx.commit().map_err(|_| UNRECOGNIZED_STATE_DB.to_string())?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_line(id: &str, provider: &str) -> String {
        format!(
            "{{\"type\":\"session_meta\",\"timestamp\":\"2026-01-01T00:00:00Z\",\"payload\":{{\"id\":\"{id}\",\"timestamp\":\"2026-01-01T00:00:00Z\",\"cwd\":\"/tmp/work\",\"model_provider\":\"{provider}\"}}}}\n{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"content\":[{{\"type\":\"input_text\",\"text\":\"hi {id}\"}}]}}}}\n"
        )
    }

    fn write_session(home: &Path, bucket: &str, id: &str, provider: &str) -> PathBuf {
        let path = home
            .join("sessions")
            .join(bucket)
            .join(format!("rollout-{id}.jsonl"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, session_line(id, provider)).unwrap();
        path
    }

    fn seed_state_db(home: &Path, rows: &[(&str, &str)]) {
        let conn = Connection::open(home.join("state_5.sqlite")).unwrap();
        conn.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, model_provider TEXT, title TEXT);",
        )
        .unwrap();
        for (id, provider) in rows {
            conn.execute(
                "INSERT INTO threads (id, model_provider, title) VALUES (?1, ?2, 't')",
                rusqlite::params![id, provider],
            )
            .unwrap();
        }
    }

    fn state_rows(home: &Path) -> Vec<(String, String)> {
        let conn = Connection::open(home.join("state_5.sqlite")).unwrap();
        let mut statement = conn
            .prepare("SELECT id, model_provider FROM threads ORDER BY id")
            .unwrap();
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap();
        rows.flatten().collect()
    }

    #[test]
    fn scan_reports_each_source_bucket_before_any_migration() {
        let home = tempfile::tempdir().unwrap();
        write_session(home.path(), "2026/01/01", "sess-deep", "deepseek");
        write_session(home.path(), "2026/01/01", "sess-open", "openai");
        seed_state_db(
            home.path(),
            &[("thread-deep", "deepseek"), ("thread-open", "openai")],
        );

        let buckets = scan_buckets(home.path(), "").unwrap();
        let deep = buckets
            .iter()
            .find(|b| b.provider_id == "deepseek")
            .unwrap();
        assert_eq!((deep.jsonl_files, deep.state_rows), (1, 1));
        let open = buckets.iter().find(|b| b.provider_id == "openai").unwrap();
        assert_eq!((open.jsonl_files, open.state_rows), (1, 1));
    }

    #[test]
    fn migration_moves_legacy_buckets_to_openai_with_ledger_and_is_idempotent() {
        let home = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        write_session(home.path(), "2026/01/01", "sess-deep", "deepseek");
        write_session(home.path(), "2026/01/01", "sess-open", "openai");
        seed_state_db(
            home.path(),
            &[("thread-deep", "deepseek"), ("thread-open", "openai")],
        );

        let sources: BTreeSet<String> = ["deepseek".to_string()].into_iter().collect();
        let outcome = migrate_to_unified(state.path(), home.path(), "", &sources).unwrap();
        assert_eq!(outcome.skipped_reason, None);
        assert_eq!(outcome.migrated_jsonl_files, 1);
        assert_eq!(outcome.migrated_state_rows, 1);

        let migrated = std::fs::read_to_string(
            home.path()
                .join("sessions/2026/01/01/rollout-sess-deep.jsonl"),
        )
        .unwrap();
        assert!(migrated.contains("\"model_provider\":\"openai\""));
        assert!(state_rows(home.path()).contains(&("thread-deep".into(), "openai".into())));

        // The ledger holds the original file and the original state DB.
        assert!(has_backup(state.path()));
        let ledger_file = std::fs::read_to_string(state.path().join(
            "backups/codex-history-unify-v1/jsonl/sessions/2026/01/01/rollout-sess-deep.jsonl",
        ))
        .unwrap();
        assert!(ledger_file.contains("\"model_provider\":\"deepseek\""));

        // Idempotent: a rerun is skipped by the directory-bound marker.
        let rerun = migrate_to_unified(state.path(), home.path(), "", &sources).unwrap();
        assert_eq!(rerun.skipped_reason, Some("already_migrated"));
    }

    #[test]
    fn restore_flips_only_ledgered_sessions_and_keeps_newer_ones() {
        let home = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        write_session(home.path(), "2026/01/01", "sess-deep", "deepseek");
        seed_state_db(home.path(), &[("thread-deep", "deepseek")]);
        let sources: BTreeSet<String> = ["deepseek".to_string()].into_iter().collect();
        migrate_to_unified(state.path(), home.path(), "", &sources).unwrap();

        // A session created after the migration must stay untouched.
        write_session(home.path(), "2026/01/02", "sess-new", "openai");
        seed_state_db_new_thread(home.path(), "thread-new");

        let outcome = restore_from_backups(state.path(), home.path(), "").unwrap();
        assert_eq!(outcome.skipped_reason, None);
        assert_eq!(outcome.restored_jsonl_files, 1);
        assert_eq!(outcome.restored_state_rows, 1);

        let restored = std::fs::read_to_string(
            home.path()
                .join("sessions/2026/01/01/rollout-sess-deep.jsonl"),
        )
        .unwrap();
        assert!(restored.contains("\"model_provider\":\"deepseek\""));
        let rows = state_rows(home.path());
        assert!(rows.contains(&("thread-deep".into(), "deepseek".into())));
        assert!(rows.contains(&("thread-new".into(), "openai".into())));
        let newer = std::fs::read_to_string(
            home.path()
                .join("sessions/2026/01/02/rollout-sess-new.jsonl"),
        )
        .unwrap();
        assert!(newer.contains("\"model_provider\":\"openai\""));
        assert!(
            !marker_path(state.path()).exists(),
            "restore clears the marker"
        );
    }

    fn seed_state_db_new_thread(home: &Path, id: &str) {
        let conn = Connection::open(home.join("state_5.sqlite")).unwrap();
        conn.execute(
            "INSERT INTO threads (id, model_provider, title) VALUES (?1, 'openai', 'new')",
            rusqlite::params![id],
        )
        .unwrap();
    }

    #[test]
    fn restore_without_a_ledger_reports_it_and_migration_without_sources_skips() {
        let home = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let outcome = restore_from_backups(state.path(), home.path(), "").unwrap();
        assert_eq!(outcome.skipped_reason, Some("no_backup_ledger"));
        let sources: BTreeSet<String> = ["openai".to_string()].into_iter().collect();
        let outcome = migrate_to_unified(state.path(), home.path(), "", &sources).unwrap();
        assert_eq!(outcome.skipped_reason, Some("no_source_provider_ids"));
    }
}
