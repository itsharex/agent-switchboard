use super::ClaudeQuotaWindow;
use serde_json::Value;

fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
}
fn reset(value: Option<&Value>) -> Option<i64> {
    value
        .and_then(Value::as_i64)
        .filter(|n| *n > 0)
        .and_then(|n| n.checked_mul(1000))
}

pub(super) fn copilot(value: &Value) -> Result<(Option<String>, Vec<ClaudeQuotaWindow>), String> {
    let snapshots = value
        .get("quota_snapshots")
        .and_then(Value::as_object)
        .ok_or("Copilot 响应缺少 quota_snapshots")?;
    let resets_at_ms = value
        .get("quota_reset_date")
        .and_then(Value::as_str)
        .and_then(|date| {
            chrono::DateTime::parse_from_rfc3339(date)
                .ok()
                .map(|date| date.timestamp_millis())
                .or_else(|| {
                    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                        .ok()
                        .and_then(|day| day.and_hms_opt(0, 0, 0))
                        .map(|date| date.and_utc().timestamp_millis())
                })
        });
    let mut windows = Vec::new();
    for (id, label) in [
        ("premium_interactions", "高级交互"),
        ("chat", "聊天"),
        ("completions", "补全"),
    ] {
        let Some(window) = snapshots.get(id).and_then(Value::as_object) else {
            continue;
        };
        let unlimited = window
            .get("unlimited")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let remaining = number(window.get("remaining"));
        let limit = number(window.get("entitlement"));
        let used_percent = if unlimited {
            None
        } else {
            number(window.get("percent_remaining"))
                .filter(|n| *n <= 100.0)
                .map(|n| 100.0 - n)
                .or_else(|| {
                    remaining
                        .zip(limit)
                        .filter(|(_, limit)| *limit > 0.0)
                        .map(|(remaining, limit)| ((limit - remaining).max(0.0) / limit) * 100.0)
                })
        };
        windows.push(ClaudeQuotaWindow {
            id: id.into(),
            label: label.into(),
            used_percent,
            remaining,
            limit,
            unlimited,
            unit: "requests",
            resets_at_ms,
        });
    }
    if windows.is_empty() {
        return Err("Copilot 未提供可识别的额度窗口，不将未知额度当作零使用".into());
    }
    Ok((
        value
            .get("copilot_plan")
            .and_then(Value::as_str)
            .map(str::to_string),
        windows,
    ))
}

pub(super) fn chatgpt(value: &Value) -> Result<(Option<String>, Vec<ClaudeQuotaWindow>), String> {
    let mut windows = Vec::new();
    for (root, prefix, label) in [
        ("rate_limit", "messages", "对话"),
        ("code_review_rate_limit", "review", "代码审查"),
    ] {
        for (key, period) in [("primary_window", "主窗口"), ("secondary_window", "次窗口")] {
            let Some(window) = value
                .get(root)
                .and_then(|rate| rate.get(key))
                .filter(|value| value.is_object())
            else {
                continue;
            };
            windows.push(ClaudeQuotaWindow {
                id: format!("{prefix}-{key}"),
                label: format!("{label} {period}"),
                used_percent: number(window.get("used_percent")),
                remaining: None,
                limit: None,
                unlimited: false,
                unit: "percent",
                resets_at_ms: reset(window.get("reset_at")),
            });
        }
    }
    if windows.is_empty() {
        return Err("ChatGPT 未提供可识别的额度窗口，不将未知额度当作零使用".into());
    }
    Ok((
        value
            .get("plan_type")
            .and_then(Value::as_str)
            .map(str::to_string),
        windows,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn copilot_unknown_and_unlimited_quotas_are_not_fabricated_as_free() {
        assert!(copilot(&json!({"quota_snapshots":{}})).is_err());
        let (_, windows) = copilot(&json!({"quota_reset_date":"2026-10-01", "copilot_plan":"pro", "quota_snapshots":{
            "premium_interactions":{"entitlement":300,"remaining":225,"percent_remaining":75,"unlimited":false},
            "chat":{"unlimited":true,"entitlement":-1,"remaining":-1}}})).unwrap();
        assert_eq!(windows[0].used_percent, Some(25.0));
        assert!(windows[0].resets_at_ms.is_some());
        assert!(windows[1].unlimited);
        assert_eq!(windows[1].used_percent, None);
    }
    #[test]
    fn chatgpt_uses_account_usage_windows_and_keeps_missing_values_unknown() {
        let (_, windows) = chatgpt(&json!({"plan_type":"plus","rate_limit":{"primary_window":{"used_percent":12,"reset_at":1750000000},"secondary_window":{}}})).unwrap();
        assert_eq!(windows[0].used_percent, Some(12.0));
        assert_eq!(windows[0].resets_at_ms, Some(1750000000000));
        assert_eq!(windows[1].used_percent, None);
    }
}
