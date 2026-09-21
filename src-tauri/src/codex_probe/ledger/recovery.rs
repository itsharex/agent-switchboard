use super::*;

impl ProbeLedger {
    // ----------------------------------------------------------- recovery

    /// Marks batches left `running` by an abrupt exit as `interrupted`,
    /// keeps every finished run, and backfills usage for runs whose session
    /// id was already saved. No run is graded after the fact.
    pub(crate) fn recover_interrupted(&self) -> Result<u32, String> {
        self.recover_interrupted_with_usage(super::super::output::read_usage)
    }

    pub(crate) fn recover_interrupted_with_usage(
        &self,
        read_usage: impl Fn(&str) -> Result<crate::codex_metering::SessionTokenSummary, String>,
    ) -> Result<u32, String> {
        let Some(mut connection) = self.open_read()? else {
            return Ok(0);
        };
        let interrupted: Vec<String> = {
            let mut statement = connection
                .prepare("SELECT id FROM probe_batches WHERE status='running'")
                .map_err(db_error)?;
            let rows = statement.query_map([], |row| row.get(0)).map_err(db_error)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?
        };
        if interrupted.is_empty() {
            return Ok(0);
        }
        let transaction = connection.transaction().map_err(db_error)?;
        for id in &interrupted {
            let orphaned: Vec<(i64, Option<String>)> = {
                let mut statement = transaction
                    .prepare("SELECT id, session_id FROM probe_runs WHERE batch_id=?1 AND status='running'")
                    .map_err(db_error)?;
                let rows = statement
                    .query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))
                    .map_err(db_error)?;
                rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?
            };
            for (run_row, session_id) in orphaned {
                let record = interrupted_run(session_id, &read_usage);
                transaction
                    .execute(
                        "UPDATE probe_runs SET status='undetermined', session_id=?2, \
                         reported_model=?3, reasoning_tokens=?4, total_tokens=?5, \
                         execution_error=?6, usage_error=?7 WHERE id=?1",
                        params![
                            run_row,
                            record.session_id,
                            record.reported_model,
                            record.reasoning_tokens.map(|value| value as i64),
                            record.total_tokens.map(|value| value as i64),
                            record.execution_error,
                            record.usage_error
                        ],
                    )
                    .map_err(db_error)?;
            }
            transaction
                .execute(
                    "UPDATE probe_batches SET status='interrupted', finished_at=?2, \
                     status_error='应用退出时检测未结束，已保留已完成的结果' WHERE id=?1",
                    params![id, now_rfc3339()],
                )
                .map_err(db_error)?;
        }
        transaction.commit().map_err(db_error)?;
        Ok(interrupted.len() as u32)
    }
}


fn interrupted_run(session_id: Option<String>,
    read_usage: &impl Fn(&str) -> Result<crate::codex_metering::SessionTokenSummary, String>,
) -> ProbeRunRecord {
    let mut record = ProbeRunRecord {
        seq: 0,
        status: ProbeRunStatus::Undetermined,
        session_id: session_id.clone(),
        final_answer: None,
        reported_model: None,
        duration_ms: None,
        reasoning_tokens: None,
        total_tokens: None,
        execution_error: Some("应用退出时检测被中断，未判定".to_string()),
        usage_error: None,
    };
    if let Some(session_id) = session_id.as_deref() {
        match read_usage(session_id) {
            Ok(summary) => {
                record.reasoning_tokens = summary.reasoning_output;
                record.total_tokens = summary.total;
                record.reported_model = summary.model;
                if summary.total.is_none() {
                    record.usage_error =
                        Some("检测会话未提供总 token 消耗".to_string());
                }
            }
            Err(message) => {
                record.usage_error = Some(format!("中断后补读用量失败：{message}"));
            }
        }
    }
    record
}
