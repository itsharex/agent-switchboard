use asb_core::contracts::{
    CodexOfficialQuota, CodexOfficialQuotaReset, CodexOfficialQuotaResetKind,
    CodexOfficialQuotaWindow,
};
use serde::{Deserialize, Serialize};

/// The label produced for the server's weekly (604800-second) window. It is
/// the only window whose usage cannot decrease without a reset.
pub(super) const WEEKLY_WINDOW_LABEL: &str = "7 天";

/// The persisted comparison baseline for after-the-fact reset detection. It
/// contains only normalized quota values plus the account marker, never a
/// credential or raw upstream payload.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CodexQuotaBaseline {
    /// Short digest of the account id; `None` when the login file omits it.
    pub(crate) account_marker: Option<String>,
    pub(crate) last_read: Option<BaselineRead>,
    pub(crate) last_reset: Option<CodexOfficialQuotaReset>,
}

/// One successful official read kept as the detection baseline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BaselineRead {
    pub(crate) at: String,
    pub(crate) windows: Vec<CodexOfficialQuotaWindow>,
}

impl CodexQuotaBaseline {
    /// Rejects malformed local baseline data without rewriting it.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(read) = &self.last_read {
            parse_timestamp(&read.at).ok_or_else(|| "基线读取时间无效".to_string())?;
            validate_windows(&read.windows)?;
        }
        if let Some(reset) = &self.last_reset {
            parse_timestamp(&reset.observed_at)
                .ok_or_else(|| "基线重置观测时间无效".to_string())?;
            if let Some(resets_at) = &reset.resets_at {
                parse_timestamp(resets_at).ok_or_else(|| "基线重置目标时间无效".to_string())?;
            }
        }
        Ok(())
    }
}

fn parse_timestamp(value: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(value).ok()
}

fn validate_windows(windows: &[CodexOfficialQuotaWindow]) -> Result<(), String> {
    for window in windows {
        if !window.used_percent.is_finite() || !(0.0..=100.0).contains(&window.used_percent) {
            return Err("基线窗口已用百分比无效".to_string());
        }
    }
    Ok(())
}

/// Compares one successful read against the persisted baseline and returns
/// the updated baseline plus the reset event this read observed.
///
/// A changed account marker wipes the history instead of reporting a
/// misleading reset. Detection watches only the weekly window: usage inside a
/// live window can only decrease when the window restarted, and a changed
/// server-declared reset time moves the schedule. `Early` requires the
/// previously declared time to still be in the future; anything else counts
/// as `Scheduled`.
pub(crate) fn apply_read(
    baseline: Option<CodexQuotaBaseline>,
    marker: Option<String>,
    quota: &CodexOfficialQuota,
    now: &str,
) -> (CodexQuotaBaseline, Option<CodexOfficialQuotaReset>) {
    let previous = baseline.unwrap_or_default();
    let account_changed = previous
        .account_marker
        .as_deref()
        .zip(marker.as_deref())
        .is_some_and(|(old, new)| old != new);

    let detected = if account_changed {
        None
    } else {
        previous
            .last_read
            .as_ref()
            .and_then(|read| detect_weekly_reset(read, quota, now))
    };

    let baseline = CodexQuotaBaseline {
        account_marker: marker,
        last_read: Some(BaselineRead {
            at: quota.at.clone().unwrap_or_else(|| now.to_string()),
            windows: quota.windows.clone(),
        }),
        last_reset: if account_changed {
            None
        } else {
            detected.clone().or(previous.last_reset)
        },
    };
    (baseline, detected)
}

fn detect_weekly_reset(
    previous: &BaselineRead,
    quota: &CodexOfficialQuota,
    now: &str,
) -> Option<CodexOfficialQuotaReset> {
    let previous_window = weekly_window(&previous.windows)?;
    let current_window = weekly_window(&quota.windows)?;

    let usage_dropped = current_window.used_percent < previous_window.used_percent;
    let schedule_moved = previous_window
        .resets_at
        .as_deref()
        .zip(current_window.resets_at.as_deref())
        .is_some_and(|(old, new)| old != new);
    if !usage_dropped && !schedule_moved {
        return None;
    }

    let early = previous_window
        .resets_at
        .as_deref()
        .and_then(parse_timestamp)
        .zip(parse_timestamp(now))
        .is_some_and(|(declared, now)| declared > now);
    Some(CodexOfficialQuotaReset {
        observed_at: quota.at.clone().unwrap_or_else(|| now.to_string()),
        kind: if early {
            CodexOfficialQuotaResetKind::Early
        } else {
            CodexOfficialQuotaResetKind::Scheduled
        },
        resets_at: current_window.resets_at.clone(),
    })
}

fn weekly_window(windows: &[CodexOfficialQuotaWindow]) -> Option<&CodexOfficialQuotaWindow> {
    windows
        .iter()
        .find(|window| window.label == WEEKLY_WINDOW_LABEL)
}
