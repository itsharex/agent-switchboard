//! Read-only Codex reset signals from Codex Runway's public status feed.
//!
//! The feed is an independent public monitor, not an OpenAI account API. This
//! module normalizes only the three facts shown in the overview and never
//! reads local credentials, session data, or account quotas.

mod feed;

#[cfg(test)]
mod tests;

use crate::probe;
use feed::{is_tibo_post_url, parse_feed, validate_timestamp, STATUS_URL};
use serde::{Deserialize, Serialize};

/// A normalized result of one explicit public reset-signal check.
///
/// This is also the complete on-disk cache contract. It deliberately contains
/// only public, already-normalized values and never a credential or raw feed
/// payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexResetStatus {
    pub source_url: String,
    pub feed_status: CodexResetFeedStatus,
    pub generated_at: String,
    pub last_successful_check_at: String,
    pub checked_at: String,
    pub latest_confirmed_signal: Option<ResetSignal>,
    pub next_scheduled_reset: Option<ResetSignal>,
    pub latest_relevant_tibo_post: Option<TiboPost>,
    pub source_warning: Option<String>,
}

/// What the overview is currently showing. `Cached` means the app has not
/// completed a newer public read in this run; it is not a claim about the
/// freshness of the external monitor itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexResetRead {
    pub status: CodexResetStatus,
    pub freshness: CodexResetFreshness,
    pub cache_warning: Option<String>,
}

impl CodexResetRead {
    pub fn cached(status: CodexResetStatus) -> Self {
        Self {
            status,
            freshness: CodexResetFreshness::Cached,
            cache_warning: None,
        }
    }

    pub fn live(status: CodexResetStatus, cache_warning: Option<String>) -> Self {
        Self {
            status,
            freshness: CodexResetFreshness::Live,
            cache_warning,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexResetFreshness {
    Cached,
    Live,
}

/// The monitor's own health declaration. It says nothing about a user's
/// Codex entitlement or quota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexResetFeedStatus {
    Ok,
    Degraded,
}

/// The public reset category reported by the source. It describes the signal,
/// never the entitlement of the signed-in account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetType {
    Global,
    Banked,
    Other,
}

/// One confirmed or scheduled reset event. Scheduled events retain their
/// precision because a date-level estimate must not look like an exact time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResetSignal {
    pub announced_at: String,
    pub effective_at: Option<String>,
    pub schedule_precision: Option<String>,
    pub confidence: f64,
    pub reset_type: ResetType,
}

/// The newest reset-related public post attributed to Tibo in this feed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TiboPost {
    pub announced_at: String,
    pub text: String,
    pub url: String,
}

impl CodexResetStatus {
    /// Rejects malformed local cache data without rewriting it. The caller can
    /// then preserve the original file for inspection and ask for a refresh.
    pub(crate) fn validate_cached(&self) -> Result<(), String> {
        if self.source_url != STATUS_URL {
            return Err("缓存的重置信号来源无效".to_string());
        }
        validate_timestamp(&self.generated_at, "生成时间")?;
        validate_timestamp(&self.last_successful_check_at, "最近成功检查时间")?;
        validate_timestamp(&self.checked_at, "本次读取时间")?;

        for (label, signal) in [
            ("确认重置信号", self.latest_confirmed_signal.as_ref()),
            ("预计重置信号", self.next_scheduled_reset.as_ref()),
        ] {
            let Some(signal) = signal else {
                continue;
            };
            validate_timestamp(&signal.announced_at, label)?;
            if let Some(effective_at) = &signal.effective_at {
                validate_timestamp(effective_at, label)?;
            }
            if !(0.0..=1.0).contains(&signal.confidence) {
                return Err("缓存的重置信号置信度无效".to_string());
            }
        }

        if let Some(post) = &self.latest_relevant_tibo_post {
            validate_timestamp(&post.announced_at, "Tibo 动态时间")?;
            if post.text.trim().is_empty() || !is_tibo_post_url(&post.url) {
                return Err("缓存的 Tibo 动态无效".to_string());
            }
        }
        Ok(())
    }
}

/// Fetches the fixed public status endpoint once. It does not poll, access X
/// directly, or infer a per-account reset state. Persisting a successful
/// normalized result is owned by the application-state layer.
pub fn check() -> Result<CodexResetStatus, String> {
    let (status, body) = probe::http_get(STATUS_URL, "Accept: application/json")?;
    if !(200..300).contains(&status) {
        return Err(format!("公开 reset feed 返回 HTTP {status}"));
    }
    parse_feed(
        &body,
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    )
}
