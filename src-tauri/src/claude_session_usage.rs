//! Claude CLI 会话用量账本（Claude 专属，与 Codex 会话同步分开实现）。
//!
//! 增量扫描 `~/.claude/projects/**/*.jsonl` 的 assistant 消息用量，按消息 id
//! 去重，按本地价格表计参考价，并与网关请求账本做 10 分钟窗口的指纹匹配以标记
//! 经本机网关的请求（避免双算）。存储与网关账本分离；这里永远不写客户端文件。

use crate::gateway::claude_pricing::ClaudePriceBook;
use crate::gateway::request_ledger::{ClaudeRequestLedger, ClaudeRequestRecord};
use asb_core::contracts::{decimal_micros, format_usd_micros, ClaudeBilling, UpstreamProtocol};
use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const FILE: &str = "claude-session-usage.json";
const BACKUP_DIR: &str = "backups/claude-session-usage";
const VERSION: u8 = 1;
const MAX_ENTRIES: usize = 20_000;
const TAIL_FINGERPRINT_BYTES: u64 = 4096;
/// Upstream matches a session message to a gateway request within this window.
const GATEWAY_MATCH_WINDOW_SECONDS: i64 = 10 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeSessionUsageRecord {
    /// `session:<message id>`; the JSONL message id is unique per response.
    pub id: String,
    pub at: String,
    pub session_id: Option<String>,
    pub model: String,
    /// Total input including both cache classes, matching the gateway ledger.
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_usd: Option<String>,
    pub price_source: Option<String>,
    /// A gateway ledger record with the same usage sits within the match
    /// window: this request already counts there.
    pub gateway_matched: bool,
    has_stop_reason: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileCursor {
    modified_nanos: i64,
    byte_offset: u64,
    tail_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LedgerFile {
    version: u8,
    cursors: BTreeMap<String, FileCursor>,
    entries: Vec<ClaudeSessionUsageRecord>,
}

impl Default for LedgerFile {
    fn default() -> Self {
        Self {
            version: VERSION,
            cursors: BTreeMap::new(),
            entries: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeSessionSyncReport {
    pub files_scanned: u32,
    pub imported: u32,
    pub updated: u32,
    pub gateway_matched: u32,
    pub pinned_rewrites: u32,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeSessionModelTotals {
    pub model: String,
    pub requests: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub priced_requests: usize,
    pub estimated_cost_usd: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeSessionUsageSummary {
    pub requests: usize,
    pub gateway_matched: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub priced_requests: usize,
    pub estimated_cost_usd: String,
    pub by_model: Vec<ClaudeSessionModelTotals>,
    pub earliest_at: Option<String>,
    pub latest_at: Option<String>,
}

fn gate() -> &'static Mutex<()> {
    static GATE: OnceLock<Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(()))
}

pub(crate) fn path(root: &Path) -> PathBuf {
    root.join(FILE)
}

fn load(root: &Path) -> Result<LedgerFile, String> {
    match crate::config_store::read_optional(&path(root)) {
        Ok(Some(text)) => {
            let file: LedgerFile = crate::config_store::parse_strict(&text)
                .map_err(|_| "Claude 会话用量账本格式无效；请备份并修复后重试".to_string())?;
            if file.version != VERSION {
                return Err("Claude 会话用量账本版本不受支持".into());
            }
            Ok(file)
        }
        Ok(None) => Ok(LedgerFile::default()),
        Err(_) => Err("Claude 会话用量账本不可读".into()),
    }
}

fn save(root: &Path, file: &LedgerFile) -> Result<(), String> {
    let text = serde_json::to_string_pretty(file).map_err(|_| "Claude 会话用量账本无法编码")?;
    crate::config_store::write_json_atomic(&path(root), &text)
        .map_err(|error| format!("无法保存 Claude 会话用量账本：{error}"))
}

fn modified_nanos(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos() as i64)
        .unwrap_or_default()
}

fn tail_fingerprint(file: &mut std::fs::File, end: u64) -> Result<String, String> {
    use sha2::Digest;
    let start = end.saturating_sub(TAIL_FINGERPRINT_BYTES);
    file.seek(SeekFrom::Start(start))
        .map_err(|error| error.to_string())?;
    let mut buffer = vec![0u8; (end - start) as usize];
    file.read_exact(&mut buffer)
        .map_err(|error| error.to_string())?;
    Ok(format!("{:x}", sha2::Sha256::digest(&buffer)))
}

/// One assistant message's usage as the JSONL line records it.
struct Candidate {
    record: ClaudeSessionUsageRecord,
}

fn candidate(value: &Value) -> Option<Candidate> {
    if value.get("type").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let message = value.get("message")?;
    let message_id = message.get("id")?.as_str()?.trim();
    if message_id.is_empty() || message_id.chars().any(char::is_control) {
        return None;
    }
    let model = message
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .unwrap_or("unknown");
    let usage = message.get("usage")?;
    let token = |key: &str| usage.get(key).and_then(Value::as_u64);
    let input = token("input_tokens")?;
    let output = token("output_tokens")?;
    let cache_read = token("cache_read_input_tokens").unwrap_or(0);
    let cache_creation = token("cache_creation_input_tokens").unwrap_or(0);
    let at = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())?;
    Some(Candidate {
        record: ClaudeSessionUsageRecord {
            id: format!("session:{message_id}"),
            at: at
                .with_timezone(&Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            session_id: value
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string),
            model: model.to_string(),
            input_tokens: input.checked_add(cache_read)?.checked_add(cache_creation)?,
            output_tokens: output,
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_creation,
            cost_usd: None,
            price_source: None,
            gateway_matched: false,
            has_stop_reason: message
                .get("stop_reason")
                .and_then(Value::as_str)
                .is_some_and(|reason| !reason.trim().is_empty()),
        },
    })
}

/// A later line for the same message id supersedes an earlier one when it
/// carries the final stop reason or a larger output count.
fn supersedes(next: &ClaudeSessionUsageRecord, current: &ClaudeSessionUsageRecord) -> bool {
    next.has_stop_reason && !current.has_stop_reason
        || next.has_stop_reason == current.has_stop_reason
            && next.output_tokens > current.output_tokens
}

fn price(book: &ClaudePriceBook, record: &mut ClaudeSessionUsageRecord) {
    let shaped = ClaudeRequestRecord {
        at: record.at.clone(),
        profile_id: None,
        route_revision: None,
        client_protocol: UpstreamProtocol::AnthropicMessages,
        upstream_protocol: None,
        request_model: None,
        mapped_model: Some(record.model.clone()),
        response_model: Some(record.model.clone()),
        cost: None,
        pricing_error: None,
        first_token_latency_ms: None,
        input_tokens: Some(record.input_tokens),
        output_tokens: Some(record.output_tokens),
        cache_read_tokens: Some(record.cache_read_tokens),
        cache_creation_tokens: Some(record.cache_creation_tokens),
        reasoning_tokens: None,
        status: Some(200),
        duration_ms: 0,
        first_byte_latency_ms: None,
        failover_attempts: Vec::new(),
    };
    let cost = book
        .estimate(&shaped, &ClaudeBilling::default())
        .ok()
        .flatten();
    record.cost_usd = cost.as_ref().map(|cost| cost.total_usd.clone());
    record.price_source = cost.map(|cost| cost.source);
}

fn gateway_matches(gateway: &[ClaudeRequestRecord], record: &ClaudeSessionUsageRecord) -> bool {
    let Ok(at) = DateTime::<FixedOffset>::parse_from_rfc3339(&record.at) else {
        return false;
    };
    gateway.iter().any(|request| {
        let Ok(request_at) = DateTime::<FixedOffset>::parse_from_rfc3339(&request.at) else {
            return false;
        };
        let model = request
            .response_model
            .as_deref()
            .or(request.mapped_model.as_deref());
        (request_at - at).num_seconds().abs() <= GATEWAY_MATCH_WINDOW_SECONDS
            && request.input_tokens == Some(record.input_tokens)
            && request.output_tokens == Some(record.output_tokens)
            && request.cache_read_tokens.unwrap_or(0) == record.cache_read_tokens
            && request.cache_creation_tokens.unwrap_or(0) == record.cache_creation_tokens
            && model.is_none_or(|model| model == record.model)
    })
}

struct Context<'a> {
    book: &'a ClaudePriceBook,
    gateway: &'a [ClaudeRequestRecord],
}

/// Reads one session file from its cursor. A truncated or rewritten prefix
/// pins the cursor to the current end without replaying: replaying would
/// double count entries that already left the file.
fn sync_file(
    file: &mut LedgerFile,
    path: &Path,
    context: &Context,
    report: &mut ClaudeSessionSyncReport,
) -> Result<(), String> {
    let key = path.to_string_lossy().to_string();
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    let modified = modified_nanos(&metadata);
    let size = metadata.len();
    let cursor = file.cursors.get(&key).cloned();
    if cursor
        .as_ref()
        .is_some_and(|cursor| cursor.modified_nanos == modified && cursor.byte_offset == size)
    {
        return Ok(());
    }
    let mut handle = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let start = match &cursor {
        Some(cursor) if cursor.byte_offset <= size => {
            if tail_fingerprint(&mut handle, cursor.byte_offset)? == cursor.tail_fingerprint {
                cursor.byte_offset
            } else {
                report.pinned_rewrites += 1;
                size
            }
        }
        Some(_) => {
            report.pinned_rewrites += 1;
            size
        }
        None => 0,
    };
    handle
        .seek(SeekFrom::Start(start))
        .map_err(|error| error.to_string())?;
    let mut consumed = start;
    let mut reader = BufReader::new(&mut handle);
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        // A line without its newline is still being written; leave it for
        // the next pass so a half-written JSON object is never parsed.
        if !line.ends_with('\n') {
            break;
        }
        consumed += read as u64;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(Candidate { mut record }) = candidate(&value) else {
            continue;
        };
        if record.input_tokens == 0 && record.output_tokens == 0 {
            continue;
        }
        price(context.book, &mut record);
        record.gateway_matched = gateway_matches(context.gateway, &record);
        match file.entries.iter_mut().find(|entry| entry.id == record.id) {
            Some(existing) => {
                if supersedes(&record, existing) {
                    *existing = record;
                    report.updated += 1;
                }
            }
            None => {
                if record.gateway_matched {
                    report.gateway_matched += 1;
                }
                file.entries.push(record);
                report.imported += 1;
            }
        }
    }
    let fingerprint = tail_fingerprint(&mut handle, consumed)?;
    file.cursors.insert(
        key,
        FileCursor {
            modified_nanos: modified,
            byte_offset: consumed,
            tail_fingerprint: fingerprint,
        },
    );
    Ok(())
}

fn prune(file: &mut LedgerFile) {
    file.entries.sort_by(|left, right| left.at.cmp(&right.at));
    if file.entries.len() > MAX_ENTRIES {
        let drop = file.entries.len() - MAX_ENTRIES;
        file.entries.drain(..drop);
    }
}

/// Approved Claude session roots only; the renderer never supplies a path.
fn claude_session_roots() -> Result<Vec<PathBuf>, String> {
    Ok(crate::session_manager::session_roots()?
        .into_iter()
        .filter(|(app, _)| *app == asb_core::contracts::AppKind::Claude)
        .map(|(_, root)| root)
        .collect())
}

pub(crate) fn sync(root: &Path) -> ClaudeSessionSyncReport {
    match claude_session_roots() {
        Ok(roots) => sync_with_roots(root, &roots),
        Err(error) => ClaudeSessionSyncReport {
            errors: vec![error],
            ..Default::default()
        },
    }
}

pub(crate) fn sync_with_roots(root: &Path, roots: &[PathBuf]) -> ClaudeSessionSyncReport {
    let _guard = gate().lock().ok();
    let mut report = ClaudeSessionSyncReport::default();
    let mut file = match load(root) {
        Ok(file) => file,
        Err(error) => {
            report.errors.push(error);
            return report;
        }
    };
    let book = match ClaudePriceBook::load(root) {
        Ok(book) => book,
        Err(error) => {
            report.errors.push(error);
            ClaudePriceBook::default()
        }
    };
    let gateway = ClaudeRequestLedger::new(root.join("claude-request-ledger.json"))
        .page(0, 200, None)
        .map(|page| page.entries)
        .unwrap_or_else(|error| {
            report
                .errors
                .push(format!("网关账本不可用，跳过去重匹配：{error}"));
            Vec::new()
        });
    let context = Context {
        book: &book,
        gateway: &gateway,
    };
    let mut files = Vec::new();
    for session_root in roots {
        match crate::session_manager::collect_session_jsonl_files(session_root) {
            Ok(mut paths) => files.append(&mut paths),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => report.errors.push("无法读取 Claude 会话目录".into()),
        }
    }
    report.files_scanned = files.len() as u32;
    for path in &files {
        if let Err(error) = sync_file(&mut file, path, &context, &mut report) {
            report.errors.push(format!(
                "Claude 会话文件解析失败 {}: {error}",
                path.display()
            ));
        }
    }
    prune(&mut file);
    if let Err(error) = save(root, &file) {
        report.errors.push(error);
    }
    report
}

pub(crate) fn summary(root: &Path) -> Result<ClaudeSessionUsageSummary, String> {
    let file = load(root)?;
    let mut by_model: BTreeMap<String, ClaudeSessionModelTotals> = BTreeMap::new();
    let mut total_cost = 0u64;
    let mut priced = 0usize;
    let mut totals = (0u64, 0u64, 0u64, 0u64);
    for entry in &file.entries {
        let bucket =
            by_model
                .entry(entry.model.clone())
                .or_insert_with(|| ClaudeSessionModelTotals {
                    model: entry.model.clone(),
                    requests: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                    priced_requests: 0,
                    estimated_cost_usd: String::new(),
                });
        bucket.requests += 1;
        bucket.input_tokens = bucket.input_tokens.saturating_add(entry.input_tokens);
        bucket.output_tokens = bucket.output_tokens.saturating_add(entry.output_tokens);
        bucket.cache_read_tokens = bucket
            .cache_read_tokens
            .saturating_add(entry.cache_read_tokens);
        bucket.cache_creation_tokens = bucket
            .cache_creation_tokens
            .saturating_add(entry.cache_creation_tokens);
        totals.0 = totals.0.saturating_add(entry.input_tokens);
        totals.1 = totals.1.saturating_add(entry.output_tokens);
        totals.2 = totals.2.saturating_add(entry.cache_read_tokens);
        totals.3 = totals.3.saturating_add(entry.cache_creation_tokens);
        if let Some(cost) = &entry.cost_usd {
            let micros = decimal_micros(cost)?;
            bucket.priced_requests += 1;
            bucket.estimated_cost_usd = format_usd_micros(
                decimal_micros(&bucket.estimated_cost_usd)
                    .unwrap_or(0)
                    .saturating_add(micros),
            );
            priced += 1;
            total_cost = total_cost.saturating_add(micros);
        }
    }
    let mut by_model: Vec<_> = by_model.into_values().collect();
    for bucket in &mut by_model {
        if bucket.estimated_cost_usd.is_empty() {
            bucket.estimated_cost_usd = format_usd_micros(0);
        }
    }
    by_model.sort_by(|left, right| right.requests.cmp(&left.requests));
    Ok(ClaudeSessionUsageSummary {
        requests: file.entries.len(),
        gateway_matched: file
            .entries
            .iter()
            .filter(|entry| entry.gateway_matched)
            .count(),
        input_tokens: totals.0,
        output_tokens: totals.1,
        cache_read_tokens: totals.2,
        cache_creation_tokens: totals.3,
        priced_requests: priced,
        estimated_cost_usd: format_usd_micros(total_cost),
        by_model,
        earliest_at: file.entries.iter().map(|entry| entry.at.clone()).min(),
        latest_at: file.entries.iter().map(|entry| entry.at.clone()).max(),
    })
}

/// Backs the ledger up, clears every entry and cursor, and rescans.
pub(crate) fn rebuild(root: &Path) -> Result<(Option<String>, ClaudeSessionSyncReport), String> {
    let roots = claude_session_roots()?;
    rebuild_with_roots(root, &roots)
}

pub(crate) fn rebuild_with_roots(
    root: &Path,
    roots: &[PathBuf],
) -> Result<(Option<String>, ClaudeSessionSyncReport), String> {
    let backup = {
        let _guard = gate().lock().ok();
        let existing = crate::config_store::read_optional(&path(root))
            .map_err(|_| "Claude 会话用量账本不可读".to_string())?;
        let backup = existing
            .map(|text| {
                let name = format!(
                    "claude-session-usage-{}.json",
                    Utc::now().format("%Y%m%dT%H%M%S%.3fZ")
                );
                crate::config_store::write_json_atomic(&root.join(BACKUP_DIR).join(&name), &text)
                    .map(|_| name)
            })
            .transpose()?;
        save(root, &LedgerFile::default())?;
        backup
    };
    Ok((backup, sync_with_roots(root, roots)))
}

#[cfg(test)]
mod tests;
