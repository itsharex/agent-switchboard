//! Incremental Codex session usage sync into the request ledger.
//!
//! Codex appends cumulative token snapshots to rollout JSONL files. This
//! engine converts them into exact per-request deltas, persists per-file
//! cursors, skips turns already recorded by the gateway, and survives forks,
//! replays, archived moves and same-mtime appends. Claude usage never passes
//! through this module.

use super::session_parse::{
    leading_thread_id_from_filename, parse_codex_file, parse_timestamp, parse_token_signature,
    thread_id_from_filename, ParentResolution, ParsedTokenEvent, TokenUsageSignature,
};
use super::store::SessionSyncCursor;
use super::{session_request_id, CodexRequestLedger, CodexRequestRecord, CodexUsageOrigin};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexSessionSyncReport {
    pub files_scanned: u32,
    pub imported: u32,
    pub skipped: u32,
    pub suspected_duplicates: u32,
    pub deferred_files: u32,
    pub errors: Vec<String>,
}

type RolloutIndex = HashMap<String, Vec<PathBuf>>;

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingReason {
    MissingParent(String),
    Stable(String),
    Retryable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingEntry {
    modified: i64,
    size: u64,
    reason: PendingReason,
}

struct ParentTimeline {
    events: Vec<(chrono::DateTime<Utc>, TokenUsageSignature)>,
    max_timestamp: Option<chrono::DateTime<Utc>>,
    has_token_without_timestamp: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParentFileStamp {
    modified: i64,
    size: u64,
}

#[derive(Default)]
struct ReplayCaches {
    parent_timelines: HashMap<PathBuf, (ParentFileStamp, ParentTimeline)>,
    replay_prefixes: HashMap<PathBuf, (i64, u64, usize)>,
    pending: HashMap<PathBuf, PendingEntry>,
}

fn replay_caches() -> &'static Mutex<ReplayCaches> {
    static CACHES: OnceLock<Mutex<ReplayCaches>> = OnceLock::new();
    CACHES.get_or_init(|| Mutex::new(ReplayCaches::default()))
}

/// Serializes sync passes (and rebuild) against each other.
fn sync_gate() -> &'static Mutex<()> {
    static GATE: OnceLock<Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(()))
}

fn modified_nanos(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos() as i64)
        .unwrap_or_default()
}

/// Approved Codex session roots only; the renderer never supplies paths.
fn codex_session_roots() -> Result<Vec<PathBuf>, String> {
    Ok(crate::session_manager::session_roots()?
        .into_iter()
        .filter(|(app, _)| *app == asb_core::contracts::AppKind::Codex)
        .map(|(_, root)| root)
        .collect())
}

fn build_rollout_index(files: &[PathBuf]) -> RolloutIndex {
    let mut index: RolloutIndex = HashMap::new();
    for path in files {
        if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
            if let Some(thread_id) = thread_id_from_filename(stem) {
                index.entry(thread_id).or_default().push(path.clone());
            }
        }
    }
    for paths in index.values_mut() {
        paths.sort();
    }
    index
}

/// Inherit the cursor of the pre-archive `sessions/` path after Codex moved
/// the file to `archived_sessions/`; request-id dedup keeps a miss harmless.
fn inherit_archived_cursor<'a>(
    path: &Path,
    path_key: &str,
    cursors: &'a HashMap<String, SessionSyncCursor>,
) -> Option<&'a SessionSyncCursor> {
    if path.parent()?.file_name()?.to_str()? != "archived_sessions" {
        return None;
    }
    let file_name = path.file_name()?.to_str()?;
    let slash_suffix = format!("/{file_name}");
    let backslash_suffix = format!("\\{file_name}");
    cursors
        .values()
        .filter(|cursor| {
            cursor.file_path != path_key
                && (cursor.file_path.ends_with(&slash_suffix)
                    || cursor.file_path.ends_with(&backslash_suffix))
        })
        .max_by_key(|cursor| (cursor.line_offset, cursor.modified_nanos))
}

fn parent_timeline(path: &Path) -> Result<ParentTimeline, String> {
    let file = File::open(path).map_err(|error| format!("无法打开父 rollout：{error}"))?;
    let stamp = {
        let metadata = file
            .metadata()
            .map_err(|error| format!("无法读取父 rollout 元数据：{error}"))?;
        ParentFileStamp {
            modified: modified_nanos(&metadata),
            size: metadata.len(),
        }
    };
    if let Ok(caches) = replay_caches().lock() {
        if let Some((cached_stamp, timeline)) = caches.parent_timelines.get(path) {
            if *cached_stamp == stamp {
                return Ok(ParentTimeline {
                    events: timeline.events.clone(),
                    max_timestamp: timeline.max_timestamp,
                    has_token_without_timestamp: timeline.has_token_without_timestamp,
                });
            }
        }
    }
    let mut events = Vec::new();
    let mut max_timestamp = None;
    let mut has_token_without_timestamp = false;
    for line in BufReader::new(&file).lines() {
        let Ok(line) = line else { continue };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let timestamp = parse_timestamp(value.get("timestamp"));
        if let Some(timestamp) = timestamp {
            max_timestamp = Some(
                max_timestamp.map_or(timestamp, |current: chrono::DateTime<Utc>| {
                    current.max(timestamp)
                }),
            );
        }
        if value.get("type").and_then(Value::as_str) != Some("event_msg")
            || value
                .get("payload")
                .and_then(|payload| payload.get("type"))
                .and_then(Value::as_str)
                != Some("token_count")
        {
            continue;
        }
        let Some(info) = value
            .get("payload")
            .and_then(|payload| payload.get("info"))
            .filter(|info| !info.is_null())
        else {
            continue;
        };
        let Some(signature) = parse_token_signature(info) else {
            continue;
        };
        match timestamp {
            Some(timestamp) => events.push((timestamp, signature)),
            None => has_token_without_timestamp = true,
        }
    }
    let timeline = ParentTimeline {
        events,
        max_timestamp,
        has_token_without_timestamp,
    };
    if let Ok(mut caches) = replay_caches().lock() {
        caches.parent_timelines.insert(
            path.to_path_buf(),
            (
                stamp,
                ParentTimeline {
                    events: timeline.events.clone(),
                    max_timestamp: timeline.max_timestamp,
                    has_token_without_timestamp: timeline.has_token_without_timestamp,
                },
            ),
        );
    }
    Ok(timeline)
}

impl ParentTimeline {
    /// Signatures the child can replay, restricted to events written before
    /// the fork. The whole parent is scanned because rollout write order does
    /// not promise monotonic timestamps.
    fn signatures_before(
        &self,
        path: &Path,
        cutoff: chrono::DateTime<Utc>,
    ) -> Result<Vec<TokenUsageSignature>, String> {
        if self.has_token_without_timestamp {
            return Err(format!(
                "父 rollout {} 的 token_count 缺少有效 timestamp",
                path.display()
            ));
        }
        if self
            .max_timestamp
            .is_none_or(|timestamp| timestamp < cutoff)
        {
            return Err(format!(
                "父 rollout {} 尚未写到 child fork 时刻",
                path.display()
            ));
        }
        Ok(self
            .events
            .iter()
            .filter(|(timestamp, _)| *timestamp <= cutoff)
            .map(|(_, signature)| signature.clone())
            .collect())
    }
}

fn resolve_parent_signatures(
    parent_id: &str,
    cutoff: chrono::DateTime<Utc>,
    rollout_index: &RolloutIndex,
) -> Result<Vec<TokenUsageSignature>, String> {
    let candidates = rollout_index
        .get(parent_id)
        .ok_or_else(|| format!("找不到父 rollout: {parent_id}"))?;
    let mut snapshots = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        snapshots.push(parent_timeline(candidate)?.signatures_before(candidate, cutoff)?);
    }
    let first = snapshots
        .first()
        .ok_or_else(|| format!("找不到父 rollout: {parent_id}"))?;
    if snapshots.iter().skip(1).any(|snapshot| snapshot != first) {
        return Err(format!(
            "父 rollout UUID {parent_id} 对应多个内容不一致的文件"
        ));
    }
    Ok(first.clone())
}

/// Longest prefix of child events already present in the parent timeline.
fn matching_replay_prefix(child: &[ParsedTokenEvent], parent: &[TokenUsageSignature]) -> usize {
    let mut parent_offset = 0usize;
    let mut matched = 0usize;
    for event in child {
        let Some(relative) = parent[parent_offset..]
            .iter()
            .position(|signature| signature == &event.signature)
        else {
            break;
        };
        parent_offset += relative + 1;
        matched += 1;
    }
    matched
}

fn mark_deferred(
    path: &Path,
    modified: i64,
    size: u64,
    reason: PendingReason,
) -> CodexSessionSyncReport {
    if let Ok(mut caches) = replay_caches().lock() {
        caches.pending.insert(
            path.to_path_buf(),
            PendingEntry {
                modified,
                size,
                reason,
            },
        );
    }
    CodexSessionSyncReport {
        deferred_files: 1,
        ..Default::default()
    }
}

struct SessionRecordDraft {
    record: CodexRequestRecord,
}

fn session_record(
    event: &ParsedTokenEvent,
    event_index: u32,
    thread_id: &str,
    session_thread_id: &str,
    fallback_at_ms: u64,
    prices: &std::collections::BTreeMap<String, super::CodexModelPrice>,
) -> SessionRecordDraft {
    let at_ms = event
        .timestamp
        .as_deref()
        .and_then(|timestamp| chrono::DateTime::parse_from_rfc3339(timestamp).ok())
        .map(|parsed| parsed.timestamp_millis().max(0) as u64)
        .unwrap_or(fallback_at_ms);
    let mut record = CodexRequestRecord {
        id: session_request_id(thread_id, event_index),
        origin: CodexUsageOrigin::Session,
        thread_id: Some(session_thread_id.to_string()),
        at_ms,
        billable: true,
        profile_id: None,
        route_revision: None,
        upstream_protocol: None,
        request_model: None,
        mapped_model: Some(event.model.clone()),
        response_model: None,
        status: Some(200),
        duration_ms: 0,
        first_byte_latency_ms: None,
        first_token_latency_ms: None,
        input_tokens: Some(event.delta.input),
        output_tokens: Some(event.delta.output),
        cache_read_tokens: Some(event.delta.cached_input),
        cache_creation_tokens: None,
        reasoning_tokens: None,
        billing: Default::default(),
        cost: None,
        pricing_error: None,
        attempts: Vec::new(),
    };
    record.cost = super::estimate(&record, prices).ok().flatten();
    SessionRecordDraft { record }
}

/// One incremental pass over every approved Codex session file.
pub(crate) fn sync_codex_session_usage(root: &Path) -> CodexSessionSyncReport {
    let _guard = sync_gate().lock().ok();
    match codex_session_roots() {
        Ok(roots) => sync_with_roots(root, &roots),
        Err(error) => CodexSessionSyncReport {
            errors: vec![error],
            ..Default::default()
        },
    }
}

/// Test/isolated entry: the caller supplies the exact approved roots.
pub(super) fn sync_with_roots(root: &Path, roots: &[PathBuf]) -> CodexSessionSyncReport {
    let mut report = CodexSessionSyncReport::default();
    let mut files = Vec::new();
    for session_root in roots {
        match crate::session_manager::collect_session_jsonl_files(&session_root) {
            Ok(mut paths) => files.append(&mut paths),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => report.errors.push("无法读取 Codex 会话目录".into()),
        }
    }
    report.files_scanned = files.len() as u32;
    let rollout_index = build_rollout_index(&files);
    let ledger = CodexRequestLedger::new(root);
    let cursors: HashMap<String, SessionSyncCursor> = match ledger.load_session_cursors() {
        Ok(cursors) => cursors.into_iter().collect(),
        Err(error) => {
            report.errors.push(error);
            return report;
        }
    };
    let prices = super::read_settings(root)
        .map(|snapshot| snapshot.settings.prices)
        .unwrap_or_default();
    let fallback_at_ms = Utc::now().timestamp_millis().max(0) as u64;

    for path in &files {
        match sync_single_file(
            &ledger,
            path,
            &rollout_index,
            &cursors,
            &prices,
            fallback_at_ms,
        ) {
            Ok(file_report) => {
                report.imported += file_report.imported;
                report.skipped += file_report.skipped;
                report.suspected_duplicates += file_report.suspected_duplicates;
                report.deferred_files += file_report.deferred_files;
            }
            Err(error) => report.errors.push(format!(
                "Codex 会话文件解析失败 {}: {error}",
                path.display()
            )),
        }
    }
    report
}

#[allow(clippy::too_many_arguments)]
fn sync_single_file(
    ledger: &CodexRequestLedger,
    path: &Path,
    rollout_index: &RolloutIndex,
    cursors: &HashMap<String, SessionSyncCursor>,
    prices: &std::collections::BTreeMap<String, super::CodexModelPrice>,
    fallback_at_ms: u64,
) -> Result<CodexSessionSyncReport, String> {
    let path_key = path.to_string_lossy().to_string();
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    let file_modified = modified_nanos(&metadata);
    let file_size = metadata.len();

    let cursor = cursors
        .get(&path_key)
        .or_else(|| inherit_archived_cursor(path, &path_key, cursors));
    let (last_modified, last_offset) = cursor
        .map(|cursor| (cursor.modified_nanos, cursor.line_offset))
        .unwrap_or((0, 0));
    let last_byte_offset = cursor.map(|cursor| cursor.byte_offset).unwrap_or(0);
    // Windows can keep mtime unchanged while Codex holds its write handle
    // open, so the byte offset participates in the change check.
    if file_modified == last_modified
        && last_byte_offset == i64::try_from(file_size).unwrap_or(i64::MIN)
    {
        return Ok(Default::default());
    }

    if let Ok(caches) = replay_caches().lock() {
        if let Some(pending) = caches.pending.get(path) {
            if pending.modified == file_modified && pending.size == file_size {
                match &pending.reason {
                    PendingReason::MissingParent(parent) if !rollout_index.contains_key(parent) => {
                        return Ok(CodexSessionSyncReport {
                            deferred_files: 1,
                            ..Default::default()
                        });
                    }
                    PendingReason::Stable(_) => {
                        return Ok(CodexSessionSyncReport {
                            deferred_files: 1,
                            ..Default::default()
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let parsed = {
        let file = File::open(path).map_err(|error| error.to_string())?;
        parse_codex_file(
            BufReader::new(file),
            (
                thread_id_from_filename(stem).as_deref(),
                leading_thread_id_from_filename(stem).as_deref(),
            ),
        )?
    };
    let new_cursor = SessionSyncCursor {
        file_path: path_key.clone(),
        modified_nanos: file_modified,
        line_offset: parsed.line_offset,
        byte_offset: parsed.observed_bytes,
    };
    if !parsed.has_billable_tokens {
        ledger.advance_session_cursor(&new_cursor)?;
        return Ok(Default::default());
    }
    let Some(root_thread_id) = parsed.root_thread_id.clone() else {
        return Ok(mark_deferred(
            path,
            file_modified,
            file_size,
            PendingReason::Stable("文件名缺少有效的尾部 UUID".to_string()),
        ));
    };
    if !parsed.root_meta_seen {
        return Ok(mark_deferred(
            path,
            file_modified,
            file_size,
            PendingReason::Stable("含计费 token 但尚无 session_meta".to_string()),
        ));
    }

    let replay_prefix = match &parsed.parent {
        ParentResolution::None => 0,
        ParentResolution::Deferred(reason) => {
            return Ok(mark_deferred(
                path,
                file_modified,
                file_size,
                PendingReason::Stable(reason.clone()),
            ));
        }
        ParentResolution::Parent(parent_id) => {
            let Some(cutoff) = parsed.root_timestamp else {
                return Ok(mark_deferred(
                    path,
                    file_modified,
                    file_size,
                    PendingReason::Stable(
                        "parented rollout 的 root meta 缺少有效 timestamp".to_string(),
                    ),
                ));
            };
            let cached = replay_caches().lock().ok().and_then(|caches| {
                caches
                    .replay_prefixes
                    .get(path)
                    .and_then(|&(modified, size, prefix)| {
                        (modified == file_modified && size == file_size).then_some(prefix)
                    })
            });
            match cached {
                Some(prefix) => prefix,
                None => match resolve_parent_signatures(parent_id, cutoff, rollout_index) {
                    Ok(parent_signatures) => {
                        let prefix =
                            matching_replay_prefix(&parsed.token_events, &parent_signatures);
                        if let Ok(mut caches) = replay_caches().lock() {
                            caches
                                .replay_prefixes
                                .insert(path.to_path_buf(), (file_modified, file_size, prefix));
                        }
                        prefix
                    }
                    Err(reason) => {
                        let pending_reason = if rollout_index.contains_key(parent_id) {
                            PendingReason::Retryable(reason)
                        } else {
                            PendingReason::MissingParent(parent_id.clone())
                        };
                        return Ok(mark_deferred(
                            path,
                            file_modified,
                            file_size,
                            pending_reason,
                        ));
                    }
                },
            }
        }
    };

    if let Ok(mut caches) = replay_caches().lock() {
        caches.pending.remove(path);
    }

    // `session_thread_id` records the root meta thread id: replacement rollouts
    // keep session identity under the leading UUID while the trailing rollout
    // id owns the request-id dedup key (event_index counts physical records).
    let session_thread_id = parsed
        .meta_thread_id
        .clone()
        .unwrap_or_else(|| root_thread_id.clone());
    let mut to_insert = Vec::new();
    let mut report = CodexSessionSyncReport::default();
    for (token_offset, event) in parsed.token_events.iter().enumerate() {
        let Some(event_index) = event.event_index else {
            continue;
        };
        if token_offset < replay_prefix {
            if event.line_offset > last_offset {
                report.skipped += 1;
            }
            continue;
        }
        if event.line_offset <= last_offset {
            continue;
        }
        to_insert.push(session_record(
            event,
            event_index,
            &root_thread_id,
            &session_thread_id,
            fallback_at_ms,
            prices,
        ));
    }

    if to_insert.is_empty() {
        ledger.advance_session_cursor(&new_cursor)?;
        return Ok(report);
    }
    let records: Vec<CodexRequestRecord> =
        to_insert.into_iter().map(|draft| draft.record).collect();
    let outcome = ledger.sync_session_entries(&records, &new_cursor)?;
    report.imported = outcome.imported;
    report.skipped += outcome.skipped;
    report.suspected_duplicates = outcome.suspected;
    Ok(report)
}

/// Backup → clear session rows and cursors → rescan. Proxy records survive.
pub(crate) fn rebuild_codex_session_usage(
    root: &Path,
) -> Result<(Option<PathBuf>, CodexSessionSyncReport), String> {
    let roots = codex_session_roots()?;
    rebuild_with_roots(root, &roots)
}

pub(super) fn rebuild_with_roots(
    root: &Path,
    roots: &[PathBuf],
) -> Result<(Option<PathBuf>, CodexSessionSyncReport), String> {
    let _guard = sync_gate()
        .lock()
        .map_err(|_| "Codex 会话同步锁不可用".to_string())?;
    let ledger = CodexRequestLedger::new(root);
    let backup = root.join("codex").join(format!(
        "requests-backup-{}.sqlite3",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    let backed_up = ledger.backup_to(&backup)?;
    ledger.clear_session_usage()?;
    if let Ok(mut caches) = replay_caches().lock() {
        *caches = ReplayCaches::default();
    }
    let report = sync_with_roots(root, roots);
    Ok((backed_up.then_some(backup), report))
}
