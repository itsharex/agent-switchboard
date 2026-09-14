//! Read-only aggregation of token usage recorded in local client sessions.
//!
//! Local records establish observed token consumption, never a provider
//! balance or remaining subscription allowance. This module does not create,
//! modify, or index a session file or SQLite database.

mod parse;

use parse::{local_date, scan_claude_session, scan_codex_session};

#[cfg(test)]
mod tests;

use asb_core::contracts::{
    AppKind, ModelUsageDay, ModelUsageGroup, ModelUsageIssue, ModelUsageRange, ModelUsageReport,
    ModelUsageTokens,
};
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TokenTotals {
    input: u64,
    cache_read: u64,
    cache_creation: u64,
    output: u64,
    total: u64,
}

impl TokenTotals {
    fn delta_from(self, previous: Option<Self>) -> Option<Self> {
        let Some(previous) = previous else {
            return Some(self);
        };
        Some(Self {
            input: self.input.checked_sub(previous.input)?,
            cache_read: self.cache_read.checked_sub(previous.cache_read)?,
            cache_creation: self.cache_creation.checked_sub(previous.cache_creation)?,
            output: self.output.checked_sub(previous.output)?,
            total: self.total.checked_sub(previous.total)?,
        })
    }

    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            input: self.input.checked_add(other.input)?,
            cache_read: self.cache_read.checked_add(other.cache_read)?,
            cache_creation: self.cache_creation.checked_add(other.cache_creation)?,
            output: self.output.checked_add(other.output)?,
            total: self.total.checked_add(other.total)?,
        })
    }

    fn into_contract(self) -> ModelUsageTokens {
        ModelUsageTokens {
            input_tokens: self.input,
            cache_read_input_tokens: self.cache_read,
            cache_creation_input_tokens: self.cache_creation,
            output_tokens: self.output,
            total_tokens: self.total,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct GroupKey {
    app: AppKind,
    model: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
struct GroupTotals {
    tokens: TokenTotals,
    session_count: u64,
}

#[derive(Debug, Clone)]
struct ClaudeUsageCandidate {
    model: String,
    tokens: TokenTotals,
    timestamp: Option<String>,
    has_stop_reason: bool,
}

#[derive(Debug, Clone, Copy)]
struct CalendarRange {
    start: Option<NaiveDate>,
    end: NaiveDate,
}

impl CalendarRange {
    fn new(range: ModelUsageRange, now: DateTime<Local>) -> Self {
        let end = now.date_naive();
        let start = match range {
            ModelUsageRange::Today => Some(end),
            ModelUsageRange::Last7Days => Some(end - Duration::days(6)),
            ModelUsageRange::Last30Days => Some(end - Duration::days(29)),
            ModelUsageRange::All => None,
        };
        Self { start, end }
    }

    fn assignment(self, timestamp: Option<&str>) -> Option<Option<NaiveDate>> {
        match timestamp.and_then(local_date) {
            Some(date)
                if self
                    .start
                    .is_none_or(|start| (start..=self.end).contains(&date)) =>
            {
                Some(Some(date))
            }
            Some(_) => None,
            None if self.start.is_none() => Some(None),
            None => None,
        }
    }
}

/// Builds a report from the exact roots whose revision owns the cache.
pub(crate) fn report_from_roots(
    range: ModelUsageRange,
    roots: &[(AppKind, std::path::PathBuf)],
    now: DateTime<Local>,
) -> ModelUsageReport {
    let calendar_range = CalendarRange::new(range, now);
    let mut groups = BTreeMap::new();
    let mut days = BTreeMap::new();
    let mut unassigned_tokens = TokenTotals::default();
    let mut issues = Vec::new();
    let mut seen_sessions = HashSet::new();

    for (app, root) in roots {
        match crate::session_manager::collect_session_jsonl_files(root) {
            Ok(paths) => {
                for path in paths {
                    match crate::session_manager::parser::session_id_in_session_file(&path) {
                        Ok(Some(session_id)) => {
                            if !seen_sessions.insert((*app, session_id)) {
                                continue;
                            }
                            scan_session_file(
                                *app,
                                &path,
                                calendar_range,
                                &mut groups,
                                &mut days,
                                &mut unassigned_tokens,
                                &mut issues,
                            );
                        }
                        Ok(None) => scan_session_file(
                            *app,
                            &path,
                            calendar_range,
                            &mut groups,
                            &mut days,
                            &mut unassigned_tokens,
                            &mut issues,
                        ),
                        Err(_) => issues.push(ModelUsageIssue {
                            app: *app,
                            message: format!("无法读取{}会话记录", (*app).label()),
                        }),
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => issues.push(ModelUsageIssue {
                app: *app,
                message: format!("无法读取{}会话目录", (*app).label()),
            }),
        }
    }

    ModelUsageReport {
        range,
        generated_at: now.with_timezone(&Utc).to_rfc3339(),
        groups: groups
            .into_iter()
            .map(|(key, totals)| ModelUsageGroup {
                app: key.app,
                model: key.model,
                input_tokens: totals.tokens.input,
                cache_read_input_tokens: totals.tokens.cache_read,
                cache_creation_input_tokens: totals.tokens.cache_creation,
                output_tokens: totals.tokens.output,
                total_tokens: totals.tokens.total,
                session_count: totals.session_count,
            })
            .collect(),
        days: days
            .into_iter()
            .map(|(date, totals)| ModelUsageDay {
                date: date.format("%F").to_string(),
                input_tokens: totals.input,
                cache_read_input_tokens: totals.cache_read,
                cache_creation_input_tokens: totals.cache_creation,
                output_tokens: totals.output,
                total_tokens: totals.total,
            })
            .collect(),
        unassigned_tokens: unassigned_tokens.into_contract(),
        issues: unique_issues(issues),
    }
}

fn scan_session_file(
    app: AppKind,
    path: &Path,
    calendar_range: CalendarRange,
    groups: &mut BTreeMap<GroupKey, GroupTotals>,
    days: &mut BTreeMap<NaiveDate, TokenTotals>,
    unassigned_tokens: &mut TokenTotals,
    issues: &mut Vec<ModelUsageIssue>,
) {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => {
            issues.push(ModelUsageIssue {
                app,
                message: format!("无法读取{}会话记录", (app).label()),
            });
            return;
        }
    };
    let mut file_groups = BTreeSet::new();
    match app {
        AppKind::Codex => scan_codex_session(
            BufReader::new(file),
            calendar_range,
            groups,
            days,
            unassigned_tokens,
            &mut file_groups,
            issues,
        ),
        AppKind::Claude => scan_claude_session(
            BufReader::new(file),
            calendar_range,
            groups,
            days,
            unassigned_tokens,
            &mut file_groups,
            issues,
        ),
    }
    for key in file_groups {
        if let Some(totals) = groups.get_mut(&key) {
            totals.session_count = totals.session_count.saturating_add(1);
        }
    }
}

fn add_tokens(
    groups: &mut BTreeMap<GroupKey, GroupTotals>,
    days: &mut BTreeMap<NaiveDate, TokenTotals>,
    unassigned_tokens: &mut TokenTotals,
    file_groups: &mut BTreeSet<GroupKey>,
    assignment: Option<NaiveDate>,
    key: GroupKey,
    tokens: TokenTotals,
) {
    let destination = match assignment {
        Some(date) => days.entry(date).or_default(),
        None => unassigned_tokens,
    };
    let Some(updated_destination) = destination.checked_add(tokens) else {
        return;
    };
    let totals = groups.entry(key.clone()).or_default();
    let Some(updated) = totals.tokens.checked_add(tokens) else {
        return;
    };
    *destination = updated_destination;
    totals.tokens = updated;
    file_groups.insert(key);
}

fn report_corruption(app: AppKind, issues: &mut Vec<ModelUsageIssue>, reported: &mut bool) {
    if *reported {
        return;
    }
    *reported = true;
    issues.push(ModelUsageIssue {
        app,
        message: format!("{}会话记录中存在无法解析的条目", (app).label()),
    });
}

fn unique_issues(issues: Vec<ModelUsageIssue>) -> Vec<ModelUsageIssue> {
    let mut seen = HashSet::new();
    issues
        .into_iter()
        .filter(|issue| seen.insert((issue.app, issue.message.clone())))
        .collect()
}
