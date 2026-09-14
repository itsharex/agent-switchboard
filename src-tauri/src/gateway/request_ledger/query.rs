//! Request filters and local-calendar spend checks; session usage is not mixed in.

use super::*;
use asb_core::contracts::{decimal_micros, ClaudeBilling};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeLedgerFilter {
    pub(crate) profile_id: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) from: Option<String>,
    pub(crate) to: Option<String>,
    #[serde(default)]
    pub(crate) failures_only: bool,
}

fn timestamp(value: &str) -> Result<DateTime<FixedOffset>, String> {
    DateTime::parse_from_rfc3339(value)
        .map_err(|_| "Claude 请求历史筛选时间必须是 RFC 3339 时间".into())
}

impl ClaudeRequestLedger {
    pub(super) fn filtered_entries(
        &self,
        filter: Option<&ClaudeLedgerFilter>,
    ) -> Result<Vec<ClaudeRequestRecord>, String> {
        let entries = self.load()?.unwrap_or_else(empty_ledger).entries;
        let Some(filter) = filter else {
            return Ok(entries);
        };
        for value in [&filter.profile_id, &filter.model].into_iter().flatten() {
            validate_text(value)?;
        }
        let from = filter.from.as_deref().map(timestamp).transpose()?;
        let to = filter.to.as_deref().map(timestamp).transpose()?;
        if from.zip(to).is_some_and(|(from, to)| from > to) {
            return Err("Claude 请求历史开始时间晚于结束时间".into());
        }
        Ok(entries
            .into_iter()
            .filter(|entry| {
                let at = timestamp(&entry.at).expect("validated ledger timestamp");
                filter
                    .profile_id
                    .as_ref()
                    .is_none_or(|id| entry.profile_id.as_ref() == Some(id))
                    && filter.model.as_ref().is_none_or(|model| {
                        [
                            &entry.request_model,
                            &entry.mapped_model,
                            &entry.response_model,
                        ]
                        .contains(&&Some(model.clone()))
                    })
                    && from.is_none_or(|from| at >= from)
                    && to.is_none_or(|to| at <= to)
                    && (!filter.failures_only || failed(entry.status))
            })
            .collect())
    }

    pub(crate) fn budget_exceeded(
        &self,
        profile: &str,
        billing: Option<&ClaudeBilling>,
    ) -> Result<Option<String>, String> {
        let Some(billing) = billing.filter(|billing| {
            billing.daily_limit_usd.is_some() || billing.monthly_limit_usd.is_some()
        }) else {
            return Ok(None);
        };
        let ledger = self.load()?.unwrap_or_else(empty_ledger);
        let today = chrono::Utc::now().date_naive().to_string();
        let (daily, monthly) = super::spend::totals(&ledger.spend_by_day_utc, profile, &today);
        for (name, spent, limit) in [
            ("当日", daily, &billing.daily_limit_usd),
            ("当月", monthly, &billing.monthly_limit_usd),
        ] {
            if let Some(limit) = limit {
                if spent >= decimal_micros(limit)? {
                    return Ok(Some(format!(
                        "Claude 供应商 UTC {name}参考费用已达到本地限额 {limit} USD"
                    )));
                }
            }
        }
        Ok(None)
    }
}
