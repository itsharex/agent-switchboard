//! Codex rollout JSONL parsing for incremental session usage.
//!
//! The parser is read-only and pure: it turns one session file into token
//! events with exact deltas plus the metadata the sync pass needs to defer,
//! inherit or de-duplicate. Writing belongs to the sync engine.

use chrono::{DateTime, Utc};
use serde_json::Value;
use std::io::{BufRead, BufReader};

/// Cumulative token counters tracked by `total_token_usage`.
#[derive(Debug, Clone, Default)]
pub(super) struct CumulativeTokens {
    pub input: u64,
    pub cached_input: u64,
    pub output: u64,
}

/// Exact per-request delta billed for one token event.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct DeltaTokens {
    pub input: u64,
    pub cached_input: u64,
    pub output: u64,
}
impl DeltaTokens {
    fn is_zero(self) -> bool {
        self.input == 0 && self.cached_input == 0 && self.output == 0
    }
}

/// Identity of one token snapshot: counters that are present, absent when the
/// writer omitted them, so repeats compare structurally instead of by zeros.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct TokenCountersSignature {
    input: Option<u64>,
    cached_input: Option<u64>,
    output: Option<u64>,
    reasoning_output: Option<u64>,
    total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct TokenUsageSignature {
    pub total: Option<TokenCountersSignature>,
    pub last: Option<TokenCountersSignature>,
}

#[derive(Debug, Clone)]
pub(super) struct ParsedTokenEvent {
    pub line_offset: i64,
    pub signature: TokenUsageSignature,
    pub delta: DeltaTokens,
    pub event_index: Option<u32>,
    pub model: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ParentResolution {
    None,
    Parent(String),
    Deferred(String),
}

#[derive(Debug)]
pub(super) struct ParsedCodexFile {
    pub root_thread_id: Option<String>,
    /// Stable logical thread from the root `session_meta`: the trailing UUID for
    /// plain files, the leading UUID for revert/resume replacement rollouts.
    pub meta_thread_id: Option<String>,
    pub root_meta_seen: bool,
    pub root_timestamp: Option<DateTime<Utc>>,
    pub parent: ParentResolution,
    pub token_events: Vec<ParsedTokenEvent>,
    pub line_offset: i64,
    /// Bytes read including an incomplete final record; used only to detect
    /// change (not as a seek position) so a partial tail can be retried later.
    pub observed_bytes: i64,
    pub has_billable_tokens: bool,
}

pub(super) fn parse_timestamp(value: Option<&Value>) -> Option<DateTime<Utc>> {
    value
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
}

pub(super) fn non_empty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn hyphenated_uuid(value: &str) -> Option<String> {
    uuid::Uuid::parse_str(value)
        .ok()
        .map(|value| value.hyphenated().to_string())
}

/// Trailing rollout UUID of the file name, the identity used for request ids.
pub(super) fn thread_id_from_filename(stem: &str) -> Option<String> {
    let candidate = stem.get(stem.len().checked_sub(36)?..)?;
    hyphenated_uuid(candidate)
}

/// Leading thread UUID of a revert/resume replacement file name
/// (`…<threadId>_<rolloutId>`); `None` for single-segment names. The root meta
/// `id` stays the original thread id, so consistency checks accept both.
pub(super) fn leading_thread_id_from_filename(stem: &str) -> Option<String> {
    let len = stem.len();
    if !stem.get(len.checked_sub(37)?..)?.starts_with('_') {
        return None;
    }
    let candidate = stem.get(len.checked_sub(73)?..len.checked_sub(37)?)?;
    hyphenated_uuid(candidate)
}

fn explicit_parent_from_meta(payload: &Value) -> ParentResolution {
    let forked_from = non_empty_string(payload.get("forked_from_id"));
    let spawned_from = payload
        .get("source")
        .and_then(|source| source.get("subagent"))
        .and_then(|subagent| subagent.get("thread_spawn"))
        .and_then(|spawn| non_empty_string(spawn.get("parent_thread_id")));
    match (forked_from, spawned_from) {
        (None, None) => ParentResolution::None,
        (Some(parent), None) | (None, Some(parent)) => ParentResolution::Parent(parent),
        (Some(forked), Some(spawned)) if forked == spawned => ParentResolution::Parent(forked),
        (Some(forked), Some(spawned)) => ParentResolution::Deferred(format!(
            "forked_from_id ({forked}) 与 thread_spawn.parent_thread_id ({spawned}) 不一致"
        )),
    }
}

/// Normalizes `GLM-4.6`/`openai/gpt-5.4-2026-03-05` style model names to a
/// stable billing key: lowercase, provider prefix and date suffixes stripped.
pub(super) fn normalize_codex_model(raw: &str) -> String {
    let mut name = raw.to_lowercase();
    if let Some(position) = name.rfind('/') {
        name = name[position + 1..].to_string();
    }
    if name.len() > 11 && name.is_char_boundary(name.len() - 11) {
        let suffix = &name[name.len() - 11..];
        let bytes = suffix.as_bytes();
        if suffix.is_ascii()
            && bytes[0] == b'-'
            && suffix[1..5].bytes().all(|byte| byte.is_ascii_digit())
            && bytes[5] == b'-'
            && suffix[6..8].bytes().all(|byte| byte.is_ascii_digit())
            && bytes[8] == b'-'
            && suffix[9..11].bytes().all(|byte| byte.is_ascii_digit())
        {
            name.truncate(name.len() - 11);
        }
    }
    if name.len() > 9 {
        let parts: Vec<&str> = name.rsplitn(2, '-').collect();
        if parts.len() == 2
            && parts[0].len() == 8
            && parts[0].bytes().all(|byte| byte.is_ascii_digit())
        {
            name = parts[1].to_string();
        }
    }
    name
}

fn parse_signature_counters(value: Option<&Value>) -> Option<TokenCountersSignature> {
    let value = value?.as_object()?;
    Some(TokenCountersSignature {
        input: value.get("input_tokens").and_then(Value::as_u64),
        cached_input: value
            .get("cached_input_tokens")
            .or_else(|| value.get("cache_read_input_tokens"))
            .and_then(Value::as_u64),
        output: value.get("output_tokens").and_then(Value::as_u64),
        reasoning_output: value.get("reasoning_output_tokens").and_then(Value::as_u64),
        total: value.get("total_tokens").and_then(Value::as_u64),
    })
}

pub(super) fn parse_token_signature(info: &Value) -> Option<TokenUsageSignature> {
    let total = parse_signature_counters(info.get("total_token_usage"));
    let last = parse_signature_counters(info.get("last_token_usage"));
    (total.is_some() || last.is_some()).then_some(TokenUsageSignature { total, last })
}

/// `rate_limits.limit_id` keys independently advancing counter lanes; a repeat
/// snapshot is only a repeat within its own lane.
fn token_snapshot_source(payload: &Value) -> Option<String> {
    payload
        .get("rate_limits")
        .and_then(|rate_limits| rate_limits.get("limit_id"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn parse_cumulative_tokens(total_usage: &Value) -> Option<CumulativeTokens> {
    let fields = total_usage.as_object()?;
    if ![
        "input_tokens",
        "cached_input_tokens",
        "cache_read_input_tokens",
        "output_tokens",
        "reasoning_output_tokens",
        "total_tokens",
    ]
    .iter()
    .any(|field| fields.contains_key(*field))
    {
        return None;
    }
    Some(CumulativeTokens {
        input: total_usage
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cached_input: total_usage
            .get("cached_input_tokens")
            .or_else(|| total_usage.get("cache_read_input_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output: total_usage
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    })
}

/// Delta between cumulative snapshots; a reset lane saturates to zero instead
/// of underflowing.
fn compute_delta(previous: &Option<CumulativeTokens>, current: &CumulativeTokens) -> DeltaTokens {
    match previous {
        None => DeltaTokens {
            input: current.input,
            cached_input: current.cached_input,
            output: current.output,
        },
        Some(previous) => DeltaTokens {
            input: current.input.saturating_sub(previous.input),
            cached_input: current.cached_input.saturating_sub(previous.cached_input),
            output: current.output.saturating_sub(previous.output),
        },
    }
}

fn update_high_water(high_water: &mut CumulativeTokens, current: &CumulativeTokens) {
    high_water.input = high_water.input.max(current.input);
    high_water.cached_input = high_water.cached_input.max(current.cached_input);
    high_water.output = high_water.output.max(current.output);
}

/// Parses one rollout file. The whole file is re-read on every change; the
/// persisted line cursor only prevents duplicate inserts, never shortens the
/// read, so truncation and rewrites stay correct. `filename_thread_ids` carries
/// the identity segments of the file name (trailing rollout UUID and, for
/// revert/resume replacements, the leading thread UUID) so the root meta can be
/// checked against both.
pub(super) fn parse_codex_file<R: std::io::Read>(
    mut reader: BufReader<R>,
    filename_thread_ids: (Option<&str>, Option<&str>),
) -> Result<ParsedCodexFile, String> {
    let (root_thread_id, leading_thread_id) = filename_thread_ids;
    let mut root_meta_seen = false;
    let mut root_timestamp = None;
    let mut meta_thread_id = None;
    let mut parent = ParentResolution::None;
    let mut current_model = "unknown".to_string();
    // `total_token_usage` is session-cumulative across model and rate-limit
    // bucket changes; divergent snapshots prefer exact `last_token_usage`.
    let mut total_high_water: Option<CumulativeTokens> = None;
    // Rate-limit refreshes can re-emit unchanged token info under another
    // `limit_id`; repeats are identified per source by the latest full
    // snapshot, cross-source repeats by the immediately preceding event.
    let mut last_signature_by_source: std::collections::HashMap<
        Option<String>,
        TokenUsageSignature,
    > = Default::default();
    let mut previous_token_signature: Option<TokenUsageSignature> = None;
    let mut event_index = 0u32;
    let mut token_events = Vec::new();
    let mut line_offset = 0i64;
    let mut observed_bytes = 0i64;
    let mut has_billable_tokens = false;

    loop {
        let mut bytes = Vec::new();
        let read = reader
            .read_until(b'\n', &mut bytes)
            .map_err(|error| format!("无法读取 Codex 会话记录：{error}"))?;
        // Count an incomplete suffix too so a crashed rollout is skipped rather
        // than fully re-inserted on every later pass; a complete final record
        // without a newline is still accepted.
        observed_bytes += read as i64;
        if read == 0
            || (bytes.last() != Some(&b'\n') && serde_json::from_slice::<Value>(&bytes).is_err())
        {
            break;
        }
        line_offset += 1;
        let Ok(line) = String::from_utf8(bytes) else {
            continue;
        };
        if !line.contains("\"event_msg\"")
            && !line.contains("\"turn_context\"")
            && !line.contains("\"session_meta\"")
        {
            continue;
        }
        if line.contains("\"event_msg\"") && !line.contains("\"token_count\"") {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("session_meta") if !root_meta_seen => {
                root_meta_seen = true;
                root_timestamp = parse_timestamp(value.get("timestamp"));
                let payload = value.get("payload").unwrap_or(&Value::Null);
                parent = explicit_parent_from_meta(payload);
                meta_thread_id = non_empty_string(
                    payload
                        .get("id")
                        .or_else(|| payload.get("thread_id"))
                        .or_else(|| payload.get("threadId")),
                )
                .map(|id| hyphenated_uuid(&id).unwrap_or(id));
                if let (Some(filename_id), Some(meta_id)) =
                    (root_thread_id, meta_thread_id.as_ref())
                {
                    let matches = filename_id == meta_id
                        || leading_thread_id.is_some_and(|id| id == meta_id.as_str());
                    if !matches {
                        parent = ParentResolution::Deferred(format!(
                            "文件名线程 ID ({filename_id}) 与 root meta ID ({meta_id}) 不一致"
                        ));
                    }
                }
                if matches!(
                    (&meta_thread_id, &parent),
                    (Some(meta_id), ParentResolution::Parent(parent_id)) if meta_id == parent_id
                ) {
                    parent = ParentResolution::Deferred(
                        "parent_thread_id 与 root meta 线程 ID 相同".to_string(),
                    );
                }
                if let ParentResolution::Parent(parent_id) = &mut parent {
                    if let Some(normalized) = hyphenated_uuid(parent_id) {
                        *parent_id = normalized;
                    } else {
                        parent = ParentResolution::Deferred(format!(
                            "显式 parent_thread_id 不是有效 UUID: {parent_id}"
                        ));
                    }
                }
            }
            Some("turn_context") => {
                if let Some(model) = value
                    .get("payload")
                    .and_then(|payload| {
                        payload
                            .get("model")
                            .or_else(|| payload.get("info").and_then(|info| info.get("model")))
                    })
                    .and_then(Value::as_str)
                {
                    current_model = normalize_codex_model(model);
                }
            }
            Some("event_msg") => {
                let Some(payload) = value.get("payload") else {
                    continue;
                };
                if payload.get("type").and_then(Value::as_str) != Some("token_count") {
                    continue;
                }
                let Some(info) = payload.get("info").filter(|info| !info.is_null()) else {
                    continue;
                };
                let Some(signature) = parse_token_signature(info) else {
                    continue;
                };
                if let Some(model) = info
                    .get("model")
                    .or_else(|| info.get("model_name"))
                    .or_else(|| payload.get("model"))
                    .and_then(Value::as_str)
                {
                    current_model = normalize_codex_model(model);
                }
                let snapshot_source = token_snapshot_source(payload);
                let total = info
                    .get("total_token_usage")
                    .and_then(parse_cumulative_tokens);
                let last = info
                    .get("last_token_usage")
                    .and_then(parse_cumulative_tokens);
                if total.is_none() && last.is_none() {
                    continue;
                }
                let has_total_snapshot = total.is_some();
                let duplicate_snapshot = has_total_snapshot
                    && (last_signature_by_source.get(&snapshot_source) == Some(&signature)
                        || previous_token_signature.as_ref() == Some(&signature));
                if has_total_snapshot {
                    last_signature_by_source.insert(snapshot_source, signature.clone());
                }
                previous_token_signature = Some(signature.clone());
                let mut delta = if duplicate_snapshot {
                    DeltaTokens::default()
                } else if let Some(last) = last {
                    // Exact per-request usage from Codex beats subtracting
                    // cumulative snapshots that may span several counter lanes.
                    DeltaTokens {
                        input: last.input,
                        cached_input: last.cached_input,
                        output: last.output,
                    }
                } else if let Some(total) = total.as_ref() {
                    compute_delta(&total_high_water, total)
                } else {
                    continue;
                };
                if let Some(total) = total {
                    if let Some(high_water) = total_high_water.as_mut() {
                        update_high_water(high_water, &total);
                    } else {
                        total_high_water = Some(total);
                    }
                }
                delta.cached_input = delta.cached_input.min(delta.input);
                let nonzero_index = if delta.is_zero() {
                    None
                } else {
                    has_billable_tokens = true;
                    event_index = event_index.saturating_add(1);
                    Some(event_index)
                };
                token_events.push(ParsedTokenEvent {
                    line_offset,
                    signature,
                    delta,
                    event_index: nonzero_index,
                    model: current_model.clone(),
                    timestamp: value
                        .get("timestamp")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                });
            }
            _ => {}
        }
    }

    Ok(ParsedCodexFile {
        root_thread_id: root_thread_id.map(str::to_owned),
        meta_thread_id,
        root_meta_seen,
        root_timestamp,
        parent,
        token_events,
        line_offset,
        observed_bytes,
        has_billable_tokens,
    })
}

/// One-shot cumulative totals for one rollout file. The degradation probe
/// consumes this directly; the sync engine keeps its own incremental state.
pub(crate) struct SessionTokenSummary {
    pub(crate) input: u64,
    pub(crate) cached_input: u64,
    pub(crate) output: u64,
    pub(crate) reasoning_output: Option<u64>,
    pub(crate) total: Option<u64>,
    pub(crate) model: Option<String>,
}

/// Reads one rollout file and returns its final cumulative usage snapshot.
/// The caller verifies the rollout's session identity before reading totals.
pub(crate) fn summarize_token_usage<R: std::io::Read>(
    reader: BufReader<R>,
) -> Result<SessionTokenSummary, String> {
    let parsed = parse_codex_file(reader, (None, None))?;
    let Some(counters) = parsed
        .token_events
        .iter()
        .rev()
        .find_map(|event| event.signature.total.clone())
    else {
        return Err("会话记录中没有 token 统计".to_string());
    };
    let model = parsed
        .token_events
        .iter()
        .rev()
        .find(|event| event.model != "unknown")
        .map(|event| event.model.clone());
    Ok(SessionTokenSummary {
        input: counters.input.unwrap_or(0),
        cached_input: counters.cached_input.unwrap_or(0),
        output: counters.output.unwrap_or(0),
        reasoning_output: counters.reasoning_output,
        total: counters.total,
        model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_reads_the_final_cumulative_snapshot() {
        let rollout = concat!(
            "{\"type\":\"session_meta\",\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"payload\":{\"id\":\"11111111-1111-1111-1111-111111111111\"}}\n",
            "{\"type\":\"event_msg\",\"timestamp\":\"2026-01-01T00:00:01.000Z\",\"payload\":{\"type\":\"token_count\",\"info\":{\"model\":\"gpt-test\",\"total_token_usage\":{\"input_tokens\":120,\"cached_input_tokens\":30,\"output_tokens\":40,\"reasoning_output_tokens\":77,\"total_tokens\":190},\"last_token_usage\":{\"input_tokens\":120,\"cached_input_tokens\":30,\"output_tokens\":40}}}}\n",
        );
        let summary =
            summarize_token_usage(BufReader::new(rollout.as_bytes())).expect("rollout summary");
        assert_eq!(summary.input, 120);
        assert_eq!(summary.cached_input, 30);
        assert_eq!(summary.output, 40);
        assert_eq!(summary.reasoning_output, Some(77));
        assert_eq!(summary.total, Some(190));
        assert_eq!(summary.model.as_deref(), Some("gpt-test"));
    }

    #[test]
    fn summarize_rejects_a_rollout_without_token_events() {
        let empty = "{\"type\":\"session_meta\",\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"payload\":{}}\n";
        assert!(summarize_token_usage(BufReader::new(empty.as_bytes())).is_err());
        assert!(summarize_token_usage(BufReader::new("".as_bytes())).is_err());
    }
}
