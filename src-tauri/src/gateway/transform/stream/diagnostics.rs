use super::*;
use crate::provider_diagnostics::{
    http_diagnostic, network_failure_kind, redact_text, ProviderDiagnostic, ProviderFailureKind,
};

impl<R: Read> SseTranscoder<R> {
    pub(crate) fn with_diagnostics(
        mut self,
        diagnostic: ProviderDiagnostic,
        secrets: &[&str],
    ) -> Self {
        self.diagnostic = Some(diagnostic);
        self.secrets = secrets.iter().map(|secret| secret.to_string()).collect();
        self
    }

    /// Validate initial output without consuming it from the response writer.
    pub(crate) fn prime(&mut self) -> Result<(), ProviderDiagnostic> {
        let mut first = [0u8; 1];
        let count = self.read(&mut first).unwrap_or_else(|error| {
            self.fail_io(&error);
            0
        });
        if self.failed || count == 0 {
            return Err(self.diagnostic.clone().unwrap_or_else(|| {
                ProviderDiagnostic::new(
                    ProviderFailureKind::StreamParse,
                    "",
                    "上游没有有效的 SSE 首帧",
                )
            }));
        }
        self.pending_offset = self.pending_offset.saturating_sub(count);
        Ok(())
    }

    pub(crate) fn failed(&self) -> bool {
        self.failed
    }

    pub(crate) fn fail_io(&mut self, error: &io::Error) {
        if let Some(error) = crate::gateway::content_encoding::stream_decode_error(error) {
            self.fail(&format!("无法解压上游 SSE：{error}"));
            return;
        }
        self.fail_kind(
            network_failure_kind(error),
            &format!("读取上游 SSE 时连接中断：{error}"),
        );
    }

    pub(super) fn fail(&mut self, message: &str) {
        self.fail_kind(ProviderFailureKind::StreamParse, message);
    }

    fn fail_kind(&mut self, kind: ProviderFailureKind, message: &str) {
        let mut diagnostic = self
            .diagnostic
            .clone()
            .unwrap_or_else(|| ProviderDiagnostic::new(kind, "", message));
        diagnostic.kind = kind;
        diagnostic.message = redact_text(
            message,
            &self.secrets.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        self.fail_with(diagnostic);
    }

    fn fail_with(&mut self, diagnostic: ProviderDiagnostic) {
        if !self.terminal {
            self.append(super::super::sse::diagnostic_event(
                self.target,
                &diagnostic,
            ));
            self.diagnostic = Some(diagnostic);
            self.failed = true;
            self.terminal = true;
        }
    }

    pub(super) fn upstream_failed(&mut self, frame: &Frame) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(&frame.data) else {
            return false;
        };
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .or(frame.event.as_deref());
        if kind == Some("response.incomplete")
            && matches!(
                &self.mode,
                StreamMode::Converting(StreamTransformer::ResponsesToAnthropic(_))
            )
        {
            // Token-limit incomplete responses are a valid terminal state for
            // Responses -> Anthropic conversion; the transformer validates
            // the reason and complete response payload below.
            return false;
        }
        if !matches!(
            kind,
            Some("error" | "response.failed" | "response.incomplete")
        ) && !value.get("error").is_some_and(|error| !error.is_null())
        {
            return false;
        }
        let mut diagnostic = http_diagnostic(
            self.diagnostic
                .as_ref()
                .map_or("", |diagnostic| &diagnostic.endpoint),
            self.diagnostic
                .as_ref()
                .and_then(|diagnostic| diagnostic.status)
                .unwrap_or(200),
            &reqwest::header::HeaderMap::new(),
            frame.data.as_bytes(),
            false,
            &self.secrets.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        diagnostic.request_id = self
            .diagnostic
            .as_ref()
            .and_then(|diagnostic| diagnostic.request_id.clone());
        diagnostic.message = "Provider 上游 SSE 返回错误".to_string();
        self.fail_with(diagnostic);
        true
    }

    pub(super) fn is_token_limit_incomplete(&self, frame: &Frame) -> bool {
        if frame.event.as_deref() != Some("response.incomplete") {
            return false;
        }
        let Ok(value) = serde_json::from_str::<Value>(&frame.data) else {
            return false;
        };
        let response = value.get("response").unwrap_or(&value);
        response.get("error").is_none_or(Value::is_null)
            && response.get("status").and_then(Value::as_str) == Some("incomplete")
            && matches!(
                response
                    .get("incomplete_details")
                    .and_then(|details| details.get("reason"))
                    .and_then(Value::as_str),
                Some("max_output_tokens" | "max_tokens")
            )
    }
}
