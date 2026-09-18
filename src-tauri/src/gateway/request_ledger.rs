//! Durable, credential-free request history for Claude gateway traffic.
//!
//! This ledger is intentionally separate from both provider-balance history
//! and in-memory gateway metrics. It stores only bounded, typed metadata that
//! can be shown or aggregated after a restart; request bodies, URLs, headers,
//! credentials, and provider response text never cross this module's API.

mod query;
mod spend;
use crate::config_store::write_json_atomic;
use asb_core::contracts::UpstreamProtocol;
use chrono::{DateTime, FixedOffset, SecondsFormat, Utc};
pub(crate) use query::ClaudeLedgerFilter;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

pub(crate) const LEDGER_VERSION: u8 = 2;
const MAX_ENTRIES: usize = 10_000;
const MAX_FAILOVER_ATTEMPTS: usize = 32;
const MAX_TEXT_LENGTH: usize = 512;

static LEDGER_MUTATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn mutation_lock() -> &'static Mutex<()> {
    LEDGER_MUTATION_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ClaudeFailoverOutcome {
    Success,
    RetryableFailure,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeFailoverAttempt {
    pub(crate) profile_id: String,
    pub(crate) route_revision: String,
    pub(crate) upstream_protocol: UpstreamProtocol,
    pub(crate) status: Option<u16>,
    pub(crate) outcome: ClaudeFailoverOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeRequestRecord {
    pub(crate) at: String,
    pub(crate) profile_id: Option<String>,
    pub(crate) route_revision: Option<String>,
    pub(crate) client_protocol: UpstreamProtocol,
    pub(crate) upstream_protocol: Option<UpstreamProtocol>,
    pub(crate) request_model: Option<String>,
    pub(crate) mapped_model: Option<String>,
    pub(crate) response_model: Option<String>,
    pub(crate) cost: Option<ClaudeRequestCost>,
    pub(crate) pricing_error: Option<String>,
    pub(crate) first_token_latency_ms: Option<u64>,
    pub(crate) input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    pub(crate) cache_read_tokens: Option<u64>,
    pub(crate) cache_creation_tokens: Option<u64>,
    pub(crate) reasoning_tokens: Option<u64>,
    pub(crate) status: Option<u16>,
    pub(crate) duration_ms: u64,
    pub(crate) first_byte_latency_ms: Option<u64>,
    #[serde(default)]
    pub(crate) failover_attempts: Vec<ClaudeFailoverAttempt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeRequestCost {
    pub(crate) model: String,
    pub(crate) source: String,
    pub(crate) multiplier: String,
    pub(crate) total_usd: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeRequestLedgerFile {
    pub(crate) version: u8,
    #[serde(default)]
    pub(crate) entries: Vec<ClaudeRequestRecord>,
    pub(crate) spend_by_day_utc: spend::DailySpend,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeRequestLedgerPage {
    pub(crate) entries: Vec<ClaudeRequestRecord>,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
    pub(crate) total: usize,
    pub(crate) has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeRequestLedgerSummary {
    pub(crate) total_requests: usize,
    pub(crate) failed_requests: usize,
    pub(crate) input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    pub(crate) cache_read_tokens: Option<u64>,
    pub(crate) cache_creation_tokens: Option<u64>,
    pub(crate) reasoning_tokens: Option<u64>,
    pub(crate) total_duration_ms: u64,
    pub(crate) average_duration_ms: Option<u64>,
    pub(crate) average_first_byte_latency_ms: Option<u64>,
    pub(crate) average_first_token_latency_ms: Option<u64>,
    pub(crate) estimated_cost_usd: String,
    pub(crate) priced_requests: usize,
    pub(crate) unpriced_requests: usize,
}

/// Process-local handle to the Claude request ledger. Multiple gateway
/// controllers can point at different test directories; the mutation lock is
/// shared so two handles can never interleave an atomic read-modify-write.
pub(crate) struct ClaudeRequestLedger {
    path: PathBuf,
}

impl ClaudeRequestLedger {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn append(&self, mut record: ClaudeRequestRecord) -> Result<(), String> {
        validate_record(&record)?;
        record.at = DateTime::<FixedOffset>::parse_from_rfc3339(&record.at)
            .map_err(|e| e.to_string())?
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Millis, true);
        let _guard = mutation_lock()
            .lock()
            .map_err(|_| "Claude 请求账本写入锁不可用".to_string())?;
        let mut ledger = self.load()?.unwrap_or_else(empty_ledger);
        spend::add(&mut ledger.spend_by_day_utc, &record)?;
        ledger.entries.push(record);
        prune(&mut ledger.entries);
        self.save(&ledger)
    }

    pub(crate) fn page(
        &self,
        offset: usize,
        limit: usize,
        filter: Option<&ClaudeLedgerFilter>,
    ) -> Result<ClaudeRequestLedgerPage, String> {
        validate_page(limit)?;
        let entries = self.filtered_entries(filter)?;
        let total = entries.len();
        let entries = entries
            .into_iter()
            .rev()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        Ok(ClaudeRequestLedgerPage {
            total,
            has_more: offset.saturating_add(entries.len()) < total,
            entries,
            offset,
            limit,
        })
    }

    pub(crate) fn summary(
        &self,
        filter: Option<&ClaudeLedgerFilter>,
    ) -> Result<ClaudeRequestLedgerSummary, String> {
        let entries = self.filtered_entries(filter)?;
        let total_requests = entries.len();
        let failed_requests = entries.iter().filter(|entry| failed(entry.status)).count();
        let total_duration_ms = entries
            .iter()
            .map(|entry| entry.duration_ms)
            .fold(0_u64, u64::saturating_add);
        let first_byte_values = entries
            .iter()
            .filter_map(|entry| entry.first_byte_latency_ms)
            .collect::<Vec<_>>();
        let (estimated_cost_usd, priced_requests) = cost_summary(&entries)?;
        Ok(ClaudeRequestLedgerSummary {
            total_requests,
            failed_requests,
            input_tokens: sum_known(&entries, |entry| entry.input_tokens),
            output_tokens: sum_known(&entries, |entry| entry.output_tokens),
            cache_read_tokens: sum_known(&entries, |entry| entry.cache_read_tokens),
            cache_creation_tokens: sum_known(&entries, |entry| entry.cache_creation_tokens),
            reasoning_tokens: sum_known(&entries, |entry| entry.reasoning_tokens),
            total_duration_ms,
            average_duration_ms: (total_requests > 0)
                .then(|| total_duration_ms / total_requests as u64),
            average_first_byte_latency_ms: average(&first_byte_values),
            average_first_token_latency_ms: average(
                &entries
                    .iter()
                    .filter_map(|entry| entry.first_token_latency_ms)
                    .collect::<Vec<_>>(),
            ),
            estimated_cost_usd,
            priced_requests,
            unpriced_requests: total_requests - priced_requests,
        })
    }

    fn load(&self) -> Result<Option<ClaudeRequestLedgerFile>, String> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("Claude 请求账本不可读：{error}")),
        };
        let mut ledger = spend::decode(&text)?;
        validate_ledger(&ledger)?;
        ledger.entries.sort_by_key(|entry| {
            DateTime::<FixedOffset>::parse_from_rfc3339(&entry.at).expect("validated ledger time")
        });
        Ok(Some(ledger))
    }

    fn save(&self, ledger: &ClaudeRequestLedgerFile) -> Result<(), String> {
        let text = serde_json::to_string_pretty(ledger)
            .map_err(|_| "Claude 请求账本序列化失败".to_string())?;
        write_json_atomic(&self.path, &text)
            .map_err(|error| format!("无法保存 Claude 请求账本：{error}"))
    }
}

pub(crate) fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn empty_ledger() -> ClaudeRequestLedgerFile {
    ClaudeRequestLedgerFile {
        version: LEDGER_VERSION,
        entries: Vec::new(),
        spend_by_day_utc: Default::default(),
    }
}

fn validate_page(limit: usize) -> Result<(), String> {
    if (1..=200).contains(&limit) {
        Ok(())
    } else {
        Err("Claude 请求账本分页大小必须是 1–200".to_string())
    }
}

fn validate_ledger(ledger: &ClaudeRequestLedgerFile) -> Result<(), String> {
    spend::validate(&ledger.spend_by_day_utc)?;
    if ledger.version != LEDGER_VERSION {
        return Err("Claude 请求账本版本不受支持；请移除旧账本后重试".to_string());
    }
    if ledger.entries.len() > MAX_ENTRIES {
        return Err("Claude 请求账本条目数量超过限制".to_string());
    }
    for entry in &ledger.entries {
        validate_record(entry)?;
    }
    Ok(())
}

fn validate_record(entry: &ClaudeRequestRecord) -> Result<(), String> {
    DateTime::<FixedOffset>::parse_from_rfc3339(&entry.at)
        .map_err(|_| "Claude 请求账本时间无效".to_string())?;
    if let Some(cost) = &entry.cost {
        asb_core::contracts::decimal_micros(&cost.total_usd)?;
        asb_core::contracts::decimal_micros(&cost.multiplier)?;
        validate_text(&cost.model)?;
        validate_text(&cost.source)?;
    }
    for value in [
        entry.profile_id.as_deref(),
        entry.route_revision.as_deref(),
        entry.request_model.as_deref(),
        entry.mapped_model.as_deref(),
        entry.response_model.as_deref(),
        entry.pricing_error.as_deref(),
    ] {
        if let Some(value) = value {
            validate_text(value)?;
        }
    }
    if let Some(status) = entry.status {
        if !(100..=599).contains(&status) {
            return Err("Claude 请求账本 HTTP 状态无效".to_string());
        }
    }
    if entry.failover_attempts.len() > MAX_FAILOVER_ATTEMPTS {
        return Err("Claude 请求账本故障转移摘要过长".to_string());
    }
    for attempt in &entry.failover_attempts {
        validate_text(&attempt.profile_id)?;
        validate_text(&attempt.route_revision)?;
        if let Some(status) = attempt.status {
            if !(100..=599).contains(&status) {
                return Err("Claude 请求账本故障转移 HTTP 状态无效".to_string());
            }
        }
    }
    Ok(())
}

fn validate_text(value: &str) -> Result<(), String> {
    if valid_text(value).is_none() {
        Err("Claude 请求账本文本字段无效".to_string())
    } else {
        Ok(())
    }
}

fn valid_text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.chars().count() <= MAX_TEXT_LENGTH
        && !value.chars().any(char::is_control))
    .then(|| value.to_string())
}

fn prune(entries: &mut Vec<ClaudeRequestRecord>) {
    entries.sort_by(|left, right| left.at.cmp(&right.at));
    if entries.len() > MAX_ENTRIES {
        let keep_from = entries.len() - MAX_ENTRIES;
        entries.drain(..keep_from);
    }
}

fn failed(status: Option<u16>) -> bool {
    status.is_none_or(|status| status >= 400)
}

fn sum_known<F>(entries: &[ClaudeRequestRecord], value: F) -> Option<u64>
where
    F: Fn(&ClaudeRequestRecord) -> Option<u64>,
{
    let mut total: Option<u64> = None;
    for entry in entries {
        if let Some(value) = value(entry) {
            total = Some(total.unwrap_or_default().saturating_add(value));
        }
    }
    total
}

fn average(values: &[u64]) -> Option<u64> {
    (!values.is_empty())
        .then(|| values.iter().copied().fold(0_u64, u64::saturating_add) / values.len() as u64)
}


fn cost_summary(entries: &[ClaudeRequestRecord]) -> Result<(String, usize), String> {
    let mut total = 0u64;
    let mut count = 0usize;
    for cost in entries.iter().filter_map(|entry| entry.cost.as_ref()) {
        total = total
            .checked_add(asb_core::contracts::decimal_micros(&cost.total_usd)?)
            .ok_or_else(|| "Claude 费用汇总超出范围".to_string())?;
        count += 1;
    }
    Ok((asb_core::contracts::format_usd_micros(total), count))
}
