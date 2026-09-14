use super::store::db_error;
use super::{CodexRequestLedger, CodexRequestRecord};
use serde::{Deserialize, Serialize};

pub(super) const WHERE: &str = "WHERE (?1 IS NULL OR profile_id=?1)
    AND (?2 IS NULL OR model=?2) AND (?3 IS NULL OR at_ms>=?3) AND (?4 IS NULL OR at_ms<?4)
    AND (?5 IS NULL OR (?5='succeeded' AND status BETWEEN 200 AND 299)
         OR (?5='failed' AND (status IS NULL OR status>=400)))";
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CodexLedgerOutcome {
    Succeeded,
    Failed,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CodexLedgerFilter {
    pub profile_id: Option<String>,
    pub model: Option<String>,
    pub from_ms: Option<u64>,
    pub until_ms: Option<u64>,
    pub outcome: Option<CodexLedgerOutcome>,
}
impl CodexLedgerFilter {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.from_ms.is_some_and(|value| value > i64::MAX as u64)
            || self.until_ms.is_some_and(|value| value > i64::MAX as u64)
            || self.from_ms.zip(self.until_ms).is_some_and(|(a, b)| a >= b)
        {
            return Err("Codex 请求查询时间范围无效".into());
        }
        if self
            .profile_id
            .as_ref()
            .is_some_and(|value| value.len() > 128)
            || self.model.as_ref().is_some_and(|value| value.len() > 512)
        {
            return Err("Codex 请求查询条件过长".into());
        }
        Ok(())
    }
    pub(super) fn params(&self) -> [rusqlite::types::Value; 5] {
        use rusqlite::types::Value;
        [
            self.profile_id.clone().map_or(Value::Null, Value::Text),
            self.model.clone().map_or(Value::Null, Value::Text),
            self.from_ms
                .map_or(Value::Null, |n| Value::Integer(n as i64)),
            self.until_ms
                .map_or(Value::Null, |n| Value::Integer(n as i64)),
            self.outcome.map_or(Value::Null, |value| {
                Value::Text(
                    match value {
                        CodexLedgerOutcome::Succeeded => "succeeded",
                        CodexLedgerOutcome::Failed => "failed",
                    }
                    .into(),
                )
            }),
        ]
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexLedgerPage {
    pub total: u64,
    pub records: Vec<CodexRequestRecord>,
}
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexLedgerSummary {
    pub requests: u64,
    pub failed_requests: u64,
    pub priced_requests: u64,
    pub unpriced_requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub reasoning_tokens: u64,
    pub estimated_usd: String,
    pub days: Vec<CodexUsageDay>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexUsageDay {
    pub start_ms: u64,
    pub requests: u64,
    pub estimated_usd: String,
    pub unpriced_requests: u64,
}
impl CodexRequestLedger {
    pub(crate) fn page(
        &self,
        filter: &CodexLedgerFilter,
        offset: u32,
        limit: u32,
    ) -> Result<CodexLedgerPage, String> {
        filter.validate()?;
        if !(1..=200).contains(&limit) {
            return Err("Codex 请求分页大小必须为 1–200".into());
        }
        let Some(connection) = self.open_read()? else {
            return Ok(CodexLedgerPage {
                total: 0,
                records: Vec::new(),
            });
        };
        let transaction = connection.unchecked_transaction().map_err(db_error)?;
        let total = transaction
            .query_row(
                &format!("SELECT count(*) FROM codex_requests {WHERE}"),
                filter.params(),
                |row| row.get(0),
            )
            .map_err(db_error)?;
        let sql = format!(
            "SELECT payload FROM codex_requests {WHERE} ORDER BY at_ms DESC,id
            LIMIT {limit} OFFSET {offset}"
        );
        let mut statement = transaction.prepare(&sql).map_err(db_error)?;
        let rows = statement
            .query_map(filter.params(), |row| row.get::<_, String>(0))
            .map_err(db_error)?;
        let records = rows
            .map(|raw| {
                let record: CodexRequestRecord = serde_json::from_str(&raw.map_err(db_error)?)
                    .map_err(|_| "Codex 请求记录格式无效，未丢弃记录".to_string())?;
                record.validate()?;
                Ok(record)
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(CodexLedgerPage { total, records })
    }
    pub(crate) fn summary(&self, filter: &CodexLedgerFilter) -> Result<CodexLedgerSummary, String> {
        filter.validate()?;
        let Some(connection) = self.open_read()? else {
            return Ok(CodexLedgerSummary {
                estimated_usd: "0.000000".into(),
                ..Default::default()
            });
        };
        let transaction = connection.unchecked_transaction().map_err(db_error)?;
        let sql = format!(
            "SELECT count(*),coalesce(sum(status IS NULL OR status>=400),0),
            count(cost_micros),coalesce(sum(input_tokens),0),coalesce(sum(output_tokens),0),
            coalesce(sum(cache_read_tokens),0),coalesce(sum(cache_creation_tokens),0),
            coalesce(sum(reasoning_tokens),0),coalesce(sum(cost_micros),0),
            coalesce(sum(cost_micros IS NULL AND json_extract(payload,'$.billable')=1),0)
            FROM codex_requests {WHERE}"
        );
        let mut summary = transaction
            .query_row(&sql, filter.params(), |row| {
                let requests: u64 = row.get(0)?;
                let priced_requests = row.get(2)?;
                Ok(CodexLedgerSummary {
                    requests,
                    failed_requests: row.get(1)?,
                    priced_requests,
                    unpriced_requests: row.get(9)?,
                    input_tokens: row.get(3)?,
                    output_tokens: row.get(4)?,
                    cache_read_tokens: row.get(5)?,
                    cache_creation_tokens: row.get(6)?,
                    reasoning_tokens: row.get(7)?,
                    estimated_usd: asb_core::contracts::format_usd_micros(row.get(8)?),
                    days: Vec::new(),
                })
            })
            .map_err(db_error)?;
        let mut statement = transaction
            .prepare(&format!(
                "SELECT at_ms/86400000*86400000,
            count(*),coalesce(sum(cost_micros),0),
            coalesce(sum(cost_micros IS NULL AND json_extract(payload,'$.billable')=1),0)
            FROM codex_requests {WHERE} GROUP BY at_ms/86400000 ORDER BY at_ms/86400000"
            ))
            .map_err(db_error)?;
        summary.days = statement
            .query_map(filter.params(), |row| {
                Ok(CodexUsageDay {
                    start_ms: row.get(0)?,
                    requests: row.get(1)?,
                    estimated_usd: asb_core::contracts::format_usd_micros(row.get(2)?),
                    unpriced_requests: row.get(3)?,
                })
            })
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        Ok(summary)
    }
}
