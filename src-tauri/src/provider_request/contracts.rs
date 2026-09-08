use crate::provider_diagnostics::{ProviderDiagnostic, ProviderFailureKind};
use asb_core::contracts::{ResponsesOptions, UpstreamProtocol};
use serde::{Deserialize, Serialize};
use std::time::Instant;

pub(super) const REQUEST_PROMPT: &str = "请只回复：连接成功。";
pub(super) const MAX_OUTPUT_TOKENS: u32 = 1_024;

// Intentionally no Debug: draft credentials must never enter diagnostics.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProviderRequestConnection {
    pub base_url: String,
    pub api_key: String,
    pub upstream_protocol: UpstreamProtocol,
    pub responses_options: Option<ResponsesOptions>,
    pub default_model: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum ProviderRequestTarget {
    Saved {
        #[serde(rename = "profileId")]
        profile_id: String,
    },
    Draft {
        connection: ProviderRequestConnection,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderRequestPreparation {
    pub request_id: String,
    pub endpoint: String,
    pub upstream_protocol: UpstreamProtocol,
    pub default_model: Option<String>,
    pub prompt: &'static str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProviderRequestInput {
    pub request_id: String,
    pub model: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProviderRequestOutcome {
    Success,
    AuthenticationFailed,
    RateLimited,
    HttpError,
    NetworkError,
    Timeout,
    InvalidResponse,
    Cancelled,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderRequestResult {
    pub outcome: ProviderRequestOutcome,
    pub status: Option<u16>,
    pub latency_ms: u64,
    /// Only the model reported by the provider, never an inferred fallback.
    pub model: Option<String>,
    pub reply: Option<String>,
    pub error: Option<String>,
    pub diagnostic: Option<ProviderDiagnostic>,
    pub at: String,
}

impl ProviderRequestResult {
    pub(super) fn failure(
        outcome: ProviderRequestOutcome,
        status: Option<u16>,
        started: Instant,
        error: impl Into<String>,
    ) -> Self {
        Self {
            outcome,
            status,
            latency_ms: started.elapsed().as_millis() as u64,
            model: None,
            reply: None,
            error: Some(error.into()),
            diagnostic: None,
            at: timestamp(),
        }
    }

    pub(super) fn cancelled(started: Instant) -> Self {
        Self::failure(
            ProviderRequestOutcome::Cancelled,
            None,
            started,
            "请求已取消；供应商可能已经处理收到的请求",
        )
    }

    pub(super) fn diagnosed(diagnostic: ProviderDiagnostic, started: Instant) -> Self {
        use ProviderFailureKind::*;
        let outcome = match diagnostic.kind {
            Authentication => ProviderRequestOutcome::AuthenticationFailed,
            RateLimit => ProviderRequestOutcome::RateLimited,
            Timeout => ProviderRequestOutcome::Timeout,
            Dns | Tls | Network => ProviderRequestOutcome::NetworkError,
            ResponseParse | StreamParse => ProviderRequestOutcome::InvalidResponse,
            _ => ProviderRequestOutcome::HttpError,
        };
        let mut result = Self::failure(
            outcome,
            diagnostic.status,
            started,
            diagnostic.message.clone(),
        );
        result.diagnostic = Some(diagnostic);
        result
    }
}

pub(super) fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
