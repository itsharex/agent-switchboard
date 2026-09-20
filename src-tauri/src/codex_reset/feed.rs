use super::{
    CodexResetFeedStatus, CodexResetHeatmap, CodexResetHeatmapDay, CodexResetStatus, ResetSignal,
    ResetType, TiboPost,
};
use serde::Deserialize;
use std::collections::HashSet;

pub(super) const STATUS_URL: &str = "https://www.codexrunway.com/api/status.json";
const STATUS_SCHEMA_VERSION: u32 = 1;
const TIBO_HANDLE: &str = "thsottiaux";
const MAX_POST_TEXT_CHARS: usize = 600;
const HEATMAP_TIMEZONE: &str = "UTC";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Feed {
    schema_version: u32,
    generated_at: String,
    last_successful_check_at: String,
    monitor: FeedMonitor,
    #[serde(default)]
    events: Vec<FeedEvent>,
    #[serde(default)]
    reset_timeline: ResetTimeline,
    #[serde(default)]
    heatmap: Option<FeedHeatmap>,
}

#[derive(Debug, Deserialize)]
struct FeedMonitor {
    status: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResetTimeline {
    next_schedule: Option<FeedEvent>,
    #[serde(default)]
    fulfilled_schedules: Vec<TimelineCompletion>,
    #[serde(default)]
    manual_completions: Vec<TimelineCompletion>,
    #[serde(default)]
    suppressed_post_ids: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimelineCompletion {
    completed_at: String,
    #[serde(default)]
    schedule: Option<FeedEvent>,
    #[serde(default)]
    schedules: Vec<FeedEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeedHeatmap {
    timezone: String,
    weeks: u32,
    total: u32,
    #[serde(default)]
    days: Vec<FeedHeatmapDay>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeedHeatmapDay {
    date: String,
    count: u32,
    level: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeedEvent {
    kind: String,
    #[serde(default)]
    reset_type: Option<String>,
    announced_at: String,
    #[serde(default)]
    effective_at: Option<String>,
    #[serde(default)]
    schedule_precision: Option<String>,
    #[serde(default)]
    confidence: f64,
    #[serde(default)]
    source: Option<FeedSource>,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeedSource {
    handle: Option<String>,
    url: Option<String>,
    #[serde(default)]
    post_id: Option<String>,
}

fn parse_timestamp(value: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(value).ok()
}

pub(super) fn validate_timestamp(value: &str, label: &str) -> Result<(), String> {
    parse_timestamp(value)
        .map(|_| ())
        .ok_or_else(|| format!("公开 feed 的{label}不是有效时间"))
}
fn reset_signal(event: &FeedEvent) -> Result<ResetSignal, String> {
    validate_timestamp(&event.announced_at, "公告时间")?;
    if let Some(effective_at) = &event.effective_at {
        validate_timestamp(effective_at, "预计时间")?;
    }
    if !(0.0..=1.0).contains(&event.confidence) {
        return Err("公开 feed 的置信度无效".to_string());
    }
    Ok(ResetSignal {
        announced_at: event.announced_at.clone(),
        effective_at: event.effective_at.clone(),
        schedule_precision: event.schedule_precision.clone(),
        confidence: event.confidence,
        reset_type: match event.reset_type.as_deref() {
            Some("global") => ResetType::Global,
            Some("banked") => ResetType::Banked,
            _ => ResetType::Other,
        },
    })
}

fn completion_signal(
    completion: &TimelineCompletion,
    event: &FeedEvent,
) -> Option<(chrono::DateTime<chrono::FixedOffset>, ResetSignal)> {
    let completed_at = parse_timestamp(&completion.completed_at)?;
    let mut signal = reset_signal(event).ok()?;
    signal.announced_at = completion.completed_at.clone();
    signal.effective_at = None;
    signal.schedule_precision = None;
    Some((completed_at, signal))
}

fn completion_events(completion: &TimelineCompletion) -> impl Iterator<Item = &FeedEvent> {
    completion
        .schedule
        .iter()
        .chain(completion.schedules.iter())
}

fn latest_confirmed_signal(feed: &Feed) -> Option<ResetSignal> {
    let direct_events = feed.events.iter().filter_map(|event| {
        (event.kind == "reset_completed")
            .then(|| parse_timestamp(&event.announced_at).zip(reset_signal(event).ok()))
            .flatten()
    });
    let timeline_events = feed
        .reset_timeline
        .fulfilled_schedules
        .iter()
        .chain(feed.reset_timeline.manual_completions.iter())
        .flat_map(|completion| {
            completion_events(completion)
                .filter_map(move |event| completion_signal(completion, event))
        });

    direct_events
        .chain(timeline_events)
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, signal)| signal)
}

/// A post id is the feed's own numeric `source.postId`; events without one
/// cannot be matched against completion or suppression entries.
fn event_post_id(event: &FeedEvent) -> Option<&str> {
    let id = event.source.as_ref()?.post_id.as_deref()?;
    (!id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())).then_some(id)
}

/// The site's forecast answer comes from the newest `reset_scheduled` event
/// that no timeline completion or suppression entry has consumed — the same
/// pending schedule `resetTimeline.nextSchedule` points at while it is
/// unfulfilled. This fallback keeps the forecast visible whenever the
/// timeline pointer is absent but the pending announcement still exists.
fn pending_scheduled_signal(feed: &Feed) -> Option<ResetSignal> {
    let consumed: HashSet<&str> = feed
        .reset_timeline
        .fulfilled_schedules
        .iter()
        .chain(&feed.reset_timeline.manual_completions)
        .flat_map(|completion| completion_events(completion).filter_map(event_post_id))
        .chain(
            feed.reset_timeline
                .suppressed_post_ids
                .iter()
                .map(String::as_str),
        )
        .collect();
    feed.events
        .iter()
        .filter(|event| {
            event.kind == "reset_scheduled"
                && event_post_id(event).is_none_or(|id| !consumed.contains(id))
        })
        .filter_map(|event| {
            parse_timestamp(&event.announced_at).zip(reset_signal(event).ok())
        })
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, signal)| signal)
}

fn heatmap(raw: &FeedHeatmap) -> Result<CodexResetHeatmap, String> {
    if raw.timezone != HEATMAP_TIMEZONE {
        return Err("公开 feed 的热力图时区不受支持".to_string());
    }
    if raw.weeks == 0 || raw.days.len() > raw.weeks as usize * 7 {
        return Err("公开 feed 的热力图周数无效".to_string());
    }
    let mut days = Vec::with_capacity(raw.days.len());
    for day in &raw.days {
        chrono::NaiveDate::parse_from_str(&day.date, "%Y-%m-%d")
            .map_err(|_| "公开 feed 的热力图日期无效".to_string())?;
        if day.level > 4 {
            return Err("公开 feed 的热力图等级无效".to_string());
        }
        days.push(CodexResetHeatmapDay {
            date: day.date.clone(),
            count: day.count,
            level: day.level,
        });
    }
    Ok(CodexResetHeatmap {
        timezone: raw.timezone.clone(),
        weeks: raw.weeks,
        total: raw.total,
        days,
    })
}

pub(super) fn is_tibo_post_url(url: &str) -> bool {
    let Some(post_id) = url.strip_prefix("https://x.com/thsottiaux/status/") else {
        return false;
    };
    !post_id.is_empty() && post_id.bytes().all(|byte| byte.is_ascii_digit())
}

fn compact_text(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match compact.char_indices().nth(MAX_POST_TEXT_CHARS) {
        Some((index, _)) => format!("{}…", &compact[..index]),
        None => compact,
    }
}

fn tibo_post(event: &FeedEvent) -> Option<TiboPost> {
    let source = event.source.as_ref()?;
    if source.handle.as_deref() != Some(TIBO_HANDLE) {
        return None;
    }
    let url = source.url.as_deref()?;
    if !is_tibo_post_url(url) || parse_timestamp(&event.announced_at).is_none() {
        return None;
    }
    let text = compact_text(&event.text);
    if text.is_empty() {
        return None;
    }
    Some(TiboPost {
        announced_at: event.announced_at.clone(),
        text,
        url: url.to_string(),
    })
}

fn latest_tibo_post<'a>(events: impl Iterator<Item = &'a FeedEvent>) -> Option<TiboPost> {
    events
        .filter_map(|event| {
            let timestamp = parse_timestamp(&event.announced_at)?;
            tibo_post(event).map(|post| (timestamp, post))
        })
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, post)| post)
}

pub(super) fn parse_feed(text: &str, checked_at: String) -> Result<CodexResetStatus, String> {
    let feed: Feed =
        serde_json::from_str(text).map_err(|_| "公开 reset feed 不是有效 JSON".to_string())?;
    if feed.schema_version != STATUS_SCHEMA_VERSION {
        return Err("公开 reset feed 的版本不受支持".to_string());
    }
    validate_timestamp(&feed.generated_at, "生成时间")?;
    validate_timestamp(&feed.last_successful_check_at, "最近成功检查时间")?;

    let latest_confirmed_signal = latest_confirmed_signal(&feed);
    let next_scheduled_reset = match feed.reset_timeline.next_schedule.as_ref() {
        Some(event) => Some(reset_signal(event)?),
        None => pending_scheduled_signal(&feed),
    };
    let feed_heatmap = heatmap(
        feed.heatmap
            .as_ref()
            .ok_or_else(|| "公开 feed 缺少热力图".to_string())?,
    )?;
    let feed_status = if feed.monitor.status == "ok" {
        CodexResetFeedStatus::Ok
    } else {
        CodexResetFeedStatus::Degraded
    };
    let source_warning = (feed_status == CodexResetFeedStatus::Degraded)
        .then(|| "公开信号源未确认正常，展示的内容可能不是最新状态。".to_string());

    Ok(CodexResetStatus {
        source_url: STATUS_URL.to_string(),
        feed_status,
        generated_at: feed.generated_at,
        last_successful_check_at: feed.last_successful_check_at,
        checked_at,
        latest_confirmed_signal,
        next_scheduled_reset,
        latest_relevant_tibo_post: latest_tibo_post(
            feed.events
                .iter()
                .chain(feed.reset_timeline.next_schedule.iter())
                .chain(
                    feed.reset_timeline
                        .fulfilled_schedules
                        .iter()
                        .chain(feed.reset_timeline.manual_completions.iter())
                        .flat_map(completion_events),
                ),
        ),
        heatmap: feed_heatmap,
        source_warning,
    })
}
