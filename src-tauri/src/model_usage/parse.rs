use super::{
    add_tokens, report_corruption, CalendarRange, ClaudeUsageCandidate, GroupKey, GroupTotals,
    TokenTotals,
};
use asb_core::contracts::{AppKind, ModelUsageIssue};
use chrono::{DateTime, Local, NaiveDate};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader};

pub(super) fn scan_codex_session(
    reader: BufReader<File>,
    calendar_range: CalendarRange,
    groups: &mut BTreeMap<GroupKey, GroupTotals>,
    days: &mut BTreeMap<NaiveDate, TokenTotals>,
    unassigned_tokens: &mut TokenTotals,
    file_groups: &mut BTreeSet<GroupKey>,
    issues: &mut Vec<ModelUsageIssue>,
) {
    let mut model = None;
    let mut previous = None;
    let mut reported_corruption = false;

    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => {
                issues.push(ModelUsageIssue {
                    app: AppKind::Codex,
                    message: "无法完整读取 Codex 会话记录".to_string(),
                });
                break;
            }
        };
        let value = match serde_json::from_str::<Value>(&line) {
            Ok(value) => value,
            Err(_) => {
                report_corruption(AppKind::Codex, issues, &mut reported_corruption);
                continue;
            }
        };
        if let Some(context_model) = codex_turn_context_model(&value) {
            model = Some(context_model);
            continue;
        }
        let Some(tokens) = codex_token_totals(&value) else {
            continue;
        };
        let delta = tokens.delta_from(previous);
        previous = Some(tokens);
        let Some(delta) = delta else {
            continue;
        };
        let Some(assignment) = calendar_range.assignment(timestamp(&value)) else {
            continue;
        };
        add_tokens(
            groups,
            days,
            unassigned_tokens,
            file_groups,
            assignment,
            GroupKey {
                app: AppKind::Codex,
                model: model.clone(),
            },
            delta,
        );
    }
}

pub(super) fn scan_claude_session(
    reader: BufReader<File>,
    calendar_range: CalendarRange,
    groups: &mut BTreeMap<GroupKey, GroupTotals>,
    days: &mut BTreeMap<NaiveDate, TokenTotals>,
    unassigned_tokens: &mut TokenTotals,
    file_groups: &mut BTreeSet<GroupKey>,
    issues: &mut Vec<ModelUsageIssue>,
) {
    let mut reported_corruption = false;
    let mut candidates = BTreeMap::new();
    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => {
                issues.push(ModelUsageIssue {
                    app: AppKind::Claude,
                    message: "无法完整读取 Claude Code 会话记录".to_string(),
                });
                break;
            }
        };
        let value = match serde_json::from_str::<Value>(&line) {
            Ok(value) => value,
            Err(_) => {
                report_corruption(AppKind::Claude, issues, &mut reported_corruption);
                continue;
            }
        };
        let Some((message_id, candidate)) = claude_usage(&value) else {
            continue;
        };
        match candidates.get(&message_id) {
            Some(current) if !prefer_claude_candidate(&candidate, current) => {}
            _ => {
                candidates.insert(message_id, candidate);
            }
        }
    }

    for candidate in candidates.into_values() {
        if candidate.tokens.total == 0 {
            continue;
        }
        let Some(assignment) = calendar_range.assignment(candidate.timestamp.as_deref()) else {
            continue;
        };
        add_tokens(
            groups,
            days,
            unassigned_tokens,
            file_groups,
            assignment,
            GroupKey {
                app: AppKind::Claude,
                model: Some(candidate.model),
            },
            candidate.tokens,
        );
    }
}

fn codex_turn_context_model(value: &Value) -> Option<String> {
    (value.get("type").and_then(Value::as_str) == Some("turn_context"))
        .then(|| value.get("payload")?.get("model")?.as_str())
        .flatten()
        .filter(|model| !model.trim().is_empty())
        .map(str::to_string)
}

fn codex_token_totals(value: &Value) -> Option<TokenTotals> {
    if value.get("type").and_then(Value::as_str) != Some("event_msg")
        || value.get("payload")?.get("type").and_then(Value::as_str) != Some("token_count")
    {
        return None;
    }
    let totals = value
        .get("payload")?
        .get("info")?
        .get("total_token_usage")?;
    let input = token_value(totals, "input_tokens")?;
    let cache_read = token_value(totals, "cached_input_tokens")?;
    let output = token_value(totals, "output_tokens")?;
    // Codex records cached input as a subset of input_tokens. The shared
    // report separates fresh input from the cache-read portion.
    let fresh_input = input.checked_sub(cache_read)?;
    Some(TokenTotals {
        input: fresh_input,
        cache_read,
        cache_creation: 0,
        output,
        total: fresh_input.checked_add(cache_read)?.checked_add(output)?,
    })
}

fn claude_usage(value: &Value) -> Option<(String, ClaudeUsageCandidate)> {
    if value.get("type").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let message = value.get("message")?;
    let message_id = message.get("id")?.as_str()?.to_string();
    let model = message
        .get("model")?
        .as_str()
        .filter(|model| !model.trim().is_empty())?
        .to_string();
    let usage = message.get("usage")?;
    let input = token_value(usage, "input_tokens")?;
    let cache_creation = optional_token_value(usage, "cache_creation_input_tokens")?;
    let cache_read = optional_token_value(usage, "cache_read_input_tokens")?;
    let output = token_value(usage, "output_tokens")?;
    let total = input
        .checked_add(cache_creation)?
        .checked_add(cache_read)?
        .checked_add(output)?;
    Some((
        message_id,
        ClaudeUsageCandidate {
            model,
            tokens: TokenTotals {
                input,
                cache_read,
                cache_creation,
                output,
                total,
            },
            timestamp: timestamp(value).map(str::to_string),
            has_stop_reason: message
                .get("stop_reason")
                .and_then(Value::as_str)
                .is_some_and(|reason| !reason.trim().is_empty()),
        },
    ))
}

fn prefer_claude_candidate(
    candidate: &ClaudeUsageCandidate,
    current: &ClaudeUsageCandidate,
) -> bool {
    candidate.has_stop_reason && !current.has_stop_reason
        || candidate.has_stop_reason == current.has_stop_reason
            && candidate.tokens.output >= current.tokens.output
}

fn token_value(value: &Value, key: &str) -> Option<u64> {
    value.get(key)?.as_u64()
}

fn optional_token_value(value: &Value, key: &str) -> Option<u64> {
    value.get(key).map_or(Some(0), Value::as_u64)
}

fn timestamp(value: &Value) -> Option<&str> {
    value.get("timestamp")?.as_str()
}

pub(super) fn local_date(timestamp: &str) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Local).date_naive())
}
