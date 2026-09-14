use super::store::db_error;
use super::{CodexBilling, CodexRequestLedger};
use asb_core::contracts::decimal_micros;
use chrono::{Datelike, NaiveDate, Utc};
use rusqlite::params;

impl CodexRequestLedger {
    /// Admission limits use completed, priced gateway requests in UTC periods.
    /// They are local spending guards, not a provider quota or a reservation system.
    pub(crate) fn check_budget(
        &self,
        profile_id: &str,
        billing: &CodexBilling,
        now_ms: u64,
    ) -> Result<(), (u16, String)> {
        billing.validate().map_err(|error| (503, error))?;
        if billing.daily_limit_usd.is_none() && billing.monthly_limit_usd.is_none() {
            return Ok(());
        }
        let now = i64::try_from(now_ms)
            .ok()
            .and_then(chrono::DateTime::<Utc>::from_timestamp_millis)
            .ok_or((503, "Codex 计量时间无效".into()))?;
        let day = now.date_naive();
        let month = NaiveDate::from_ymd_opt(day.year(), day.month(), 1)
            .ok_or((503, "Codex 计量月份无效".into()))?;
        let Some(connection) = self.open_read().map_err(|error| (503, error))? else {
            return Ok(());
        };
        let transaction = connection
            .unchecked_transaction()
            .map_err(|error| (503, db_error(error)))?;
        for (date, limit, label) in [
            (day, &billing.daily_limit_usd, "日"),
            (month, &billing.monthly_limit_usd, "月"),
        ] {
            let Some(limit) = limit else {
                continue;
            };
            let from = date
                .and_hms_opt(0, 0, 0)
                .ok_or((503, "Codex 计量日期无效".into()))?
                .and_utc()
                .timestamp_millis();
            let (micros, unknown): (u64, u64) = transaction
                .query_row(
                    "SELECT coalesce(sum(cost_micros),0),coalesce(sum(cost_micros IS NULL
                 AND json_extract(payload,'$.billable')=1 AND (status BETWEEN 200 AND 299
                      OR input_tokens>0 OR output_tokens>0)),0)
                 FROM codex_requests WHERE profile_id=?1 AND at_ms>=?2 AND at_ms<=?3",
                    params![profile_id, from, now_ms],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|error| (503, db_error(error)))?;
            if unknown > 0 {
                return Err((503, format!("Codex 本{label}存在 {unknown} 条费用未知的请求，请补充价格并回填后再使用限额")));
            }
            if micros >= decimal_micros(limit).map_err(|error| (503, error))? {
                return Err((
                    429,
                    format!("Codex 本地{label}限额已达到；可调整限额或切换供应商"),
                ));
            }
        }
        Ok(())
    }
}
