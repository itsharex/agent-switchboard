//! Claude ledger persistence; Codex accounting is a separate recorder.
use super::*;
impl RequestSpan {
    pub(super) fn finish_claude(
        self,
        status: Option<u16>,
        duration_ms: u64,
        first_byte_latency_ms: Option<u64>,
    ) {
        let Some(ledger) = self.ledger else {
            return;
        };
        if self.app != AppKind::Claude {
            return;
        }
        let mut record = ClaudeRequestRecord {
            at: crate::gateway::request_ledger::now(),
            profile_id: self.profile_id,
            route_revision: self.route_revision,
            client_protocol: self.client_protocol,
            upstream_protocol: self.upstream_protocol,
            request_model: self.request_model,
            mapped_model: self.mapped_model,
            response_model: self.response_model,
            cost: None,
            pricing_error: None,
            first_token_latency_ms: self
                .first_token_at
                .map(|at| at.duration_since(self.started).as_millis() as u64),
            input_tokens: self.usage.input_tokens,
            output_tokens: self.usage.output_tokens,
            cache_read_tokens: self.usage.cache_read_tokens,
            cache_creation_tokens: self.usage.cache_creation_tokens,
            reasoning_tokens: self.usage.reasoning_tokens,
            status,
            duration_ms,
            first_byte_latency_ms,
            failover_attempts: self.failover_attempts,
        };
        if let Some(prices) = self.prices {
            match prices.and_then(|prices| prices.estimate(&record, &self.billing)) {
                Ok(cost) => record.cost = cost,
                Err(error) => record.pricing_error = Some(error),
            }
        }
        let secrets: Vec<&str> = self.secrets.iter().map(String::as_str).collect();
        for field in [
            &mut record.request_model,
            &mut record.mapped_model,
            &mut record.response_model,
            &mut record.pricing_error,
        ] {
            if let Some(value) = field {
                *value = crate::provider_diagnostics::redact_text(value, &secrets);
            }
        }
        if let Err(error) = ledger.append(record) {
            log::warn!("无法保存 Claude 请求账本：{error}");
        }
    }
}
