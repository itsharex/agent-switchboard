//! Codex request accounting has no dependency on Claude billing or storage.
use super::*;
use crate::codex_metering::{
    self, CodexBilling, CodexMeteringSettings, CodexRequestAttempt, CodexRequestLedger,
    CodexRequestRecord,
};
use crate::gateway::ActiveRoute;
use std::path::Path;

pub(super) struct CodexRecording {
    ledger: CodexRequestLedger,
    settings: Result<CodexMeteringSettings, String>,
    billing: CodexBilling,
    id: String,
    at_ms: u64,
    billable: bool,
    attempts: Vec<CodexRequestAttempt>,
}
impl RequestSpan {
    pub(crate) fn start_codex(metrics: Arc<GatewayMetrics>, root: &Path) -> Self {
        let mut span = Self::start(metrics, AppKind::Codex, UpstreamProtocol::Responses);
        span.codex = Some(CodexRecording {
            ledger: CodexRequestLedger::new(root),
            settings: codex_metering::read_settings(root).map(|snapshot| snapshot.settings),
            billing: Default::default(),
            id: uuid::Uuid::new_v4().to_string(),
            at_ms: unix_now_ms(),
            billable: false,
            attempts: Vec::new(),
        });
        span
    }
    pub(crate) fn admit_codex_route(
        &mut self,
        route: &ActiveRoute,
        billable: bool,
    ) -> Result<(), (u16, String)> {
        self.protect_secrets(&[&route.api_key, &route.client_token]);
        let Some(recorder) = &mut self.codex else {
            return Ok(());
        };
        let settings = recorder
            .settings
            .as_ref()
            .map_err(|error| (503, error.clone()))?;
        recorder.billing = settings
            .providers
            .get(&route.profile_id)
            .cloned()
            .unwrap_or_default();
        if billable {
            recorder
                .ledger
                .check_budget(&route.profile_id, &recorder.billing, unix_now_ms())?;
        }
        Ok(())
    }
    /// Mark only requests actually sent upstream; quota checks and local model lists are free.
    pub(crate) fn note_codex_attempt(
        &mut self,
        route: &ActiveRoute,
        status: Option<u16>,
        retryable: bool,
    ) {
        let Some(recorder) = &mut self.codex else {
            return;
        };
        recorder.billable = true;
        if recorder.attempts.len() < 32 {
            recorder.attempts.push(CodexRequestAttempt {
                profile_id: route.profile_id.clone(),
                route_revision: route.fingerprint.clone(),
                upstream_protocol: route.upstream_protocol,
                status,
                retryable,
            });
        }
    }
    pub(super) fn finish_codex(
        self,
        status: Option<u16>,
        duration_ms: u64,
        first_byte_latency_ms: Option<u64>,
    ) {
        let Some(recorder) = self.codex else {
            return;
        };
        if status == Some(101) {
            return;
        }
        let mut record = CodexRequestRecord {
            id: recorder.id,
            at_ms: recorder.at_ms,
            billable: recorder.billable,
            profile_id: self.profile_id,
            route_revision: self.route_revision,
            upstream_protocol: self.upstream_protocol,
            request_model: self.request_model,
            mapped_model: self.mapped_model,
            response_model: self.response_model,
            status,
            duration_ms,
            first_byte_latency_ms,
            first_token_latency_ms: self
                .first_token_at
                .map(|at| at.duration_since(self.started).as_millis() as u64),
            input_tokens: self.usage.input_tokens,
            output_tokens: self.usage.output_tokens,
            cache_read_tokens: self.usage.cache_read_tokens,
            cache_creation_tokens: self.usage.cache_creation_tokens,
            reasoning_tokens: self.usage.reasoning_tokens,
            billing: recorder.billing,
            cost: None,
            pricing_error: None,
            attempts: recorder.attempts,
        };
        if let Some(attempt) = record.attempts.last_mut() {
            if attempt
                .status
                .is_none_or(|status| (200..=299).contains(&status))
            {
                attempt.status = status;
            }
        }
        match recorder
            .settings
            .and_then(|settings| codex_metering::estimate(&record, &settings.prices))
        {
            Ok(cost) => record.cost = cost,
            Err(error) => record.pricing_error = Some(error),
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
                *value = value
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(512)
                    .collect();
            }
        }
        if let Some(cost) = &mut record.cost {
            cost.model = crate::provider_diagnostics::redact_text(&cost.model, &secrets);
        }
        if let Err(error) = recorder.ledger.append(&record) {
            log::warn!("无法保存 Codex 请求账本：{error}");
        }
    }
}
