//! Versioned cumulative spend survives pruning of detailed request history.

use super::*;
use asb_core::contracts::decimal_micros;
use std::collections::BTreeMap;

pub(super) type DailySpend = BTreeMap<String, BTreeMap<String, u64>>;
const MAX_SPEND_DAYS: usize = 400;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyLedger {
    version: u8,
    #[serde(default)]
    entries: Vec<ClaudeRequestRecord>,
}

pub(super) fn decode(text: &str) -> Result<ClaudeRequestLedgerFile, String> {
    let bad = || "Claude 请求账本格式无效；原文件未改写，请备份并修复后重试".to_string();
    let value: Value = serde_json::from_str(text).map_err(|_| bad())?;
    match value.get("version").and_then(Value::as_u64) {
        Some(1) => {
            let old: LegacyLedger = serde_json::from_value(value).map_err(|_| bad())?;
            debug_assert_eq!(old.version, 1);
            let mut result = empty_ledger();
            for entry in &old.entries {
                validate_record(entry)?;
                add(&mut result.spend_by_day_utc, entry)?;
            }
            result.entries = old.entries;
            Ok(result)
        }
        Some(version) if version == u64::from(LEDGER_VERSION) => {
            serde_json::from_value(value).map_err(|_| bad())
        }
        _ => Err("Claude 请求账本版本不受支持；原文件未改写".into()),
    }
}

pub(super) fn add(spend: &mut DailySpend, entry: &ClaudeRequestRecord) -> Result<(), String> {
    let (Some(profile), Some(cost)) = (&entry.profile_id, &entry.cost) else {
        return Ok(());
    };
    let day = DateTime::<FixedOffset>::parse_from_rfc3339(&entry.at)
        .map_err(|e| e.to_string())?
        .with_timezone(&Utc)
        .date_naive()
        .to_string();
    let total = spend
        .entry(day)
        .or_default()
        .entry(profile.clone())
        .or_default();
    *total = total
        .checked_add(decimal_micros(&cost.total_usd)?)
        .ok_or_else(|| "Claude 费用累计超出范围".to_string())?;
    while spend.len() > MAX_SPEND_DAYS {
        spend.pop_first();
    }
    Ok(())
}

pub(super) fn validate(spend: &DailySpend) -> Result<(), String> {
    if spend.len() > MAX_SPEND_DAYS {
        return Err("Claude 费用累计天数超出范围".into());
    }
    for (day, profiles) in spend {
        chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .map_err(|_| "Claude 费用累计日期无效".to_string())?;
        for profile in profiles.keys() {
            validate_text(profile)?;
        }
    }
    Ok(())
}

pub(super) fn totals(spend: &DailySpend, profile: &str, today: &str) -> (u64, u64) {
    let daily = spend
        .get(today)
        .and_then(|profiles| profiles.get(profile))
        .copied()
        .unwrap_or(0);
    let month = &today[..7];
    let monthly = spend
        .iter()
        .filter(|(day, _)| day.starts_with(month))
        .filter_map(|(_, profiles)| profiles.get(profile))
        .fold(0u64, |sum, cost| sum.saturating_add(*cost));
    (daily, monthly)
}
