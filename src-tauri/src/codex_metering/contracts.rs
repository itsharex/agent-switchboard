use asb_core::contracts::UpstreamProtocol;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CodexPricingModelSource {
    #[default]
    Request,
    Response,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CodexBilling {
    pub cost_multiplier: String,
    pub model_source: CodexPricingModelSource,
    pub daily_limit_usd: Option<String>,
    pub monthly_limit_usd: Option<String>,
}
impl Default for CodexBilling {
    fn default() -> Self {
        Self {
            cost_multiplier: "1".into(),
            model_source: Default::default(),
            daily_limit_usd: None,
            monthly_limit_usd: None,
        }
    }
}
impl CodexBilling {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let multiplier = asb_core::contracts::decimal_micros(&self.cost_multiplier)?;
        if multiplier > 1_000_000_000 {
            return Err("Codex 成本倍率不能超过 1000".into());
        }
        for limit in [&self.daily_limit_usd, &self.monthly_limit_usd]
            .into_iter()
            .flatten()
        {
            if asb_core::contracts::decimal_micros(limit)? == 0 {
                return Err("Codex 日/月限额必须大于零；停用请清除限额".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CodexRequestCost {
    pub model: String,
    pub source: String,
    pub multiplier: String,
    pub total_usd: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CodexRequestAttempt {
    pub profile_id: String,
    pub route_revision: String,
    pub upstream_protocol: UpstreamProtocol,
    pub status: Option<u16>,
    pub retryable: bool,
}

/// Gateway-recorded requests versus usage recovered from local session files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CodexUsageOrigin {
    #[default]
    Proxy,
    Session,
}
impl CodexUsageOrigin {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Proxy => "proxy",
            Self::Session => "session",
        }
    }
}

/// `codex-session-v1:<thread uuid>:<event index>`; the owner of session row identity.
pub(crate) const CODEX_SESSION_ID_PREFIX: &str = "codex-session-v1";

#[expect(dead_code)] // reserved: session-usage sync slate
pub(crate) fn session_request_id(thread_id: &str, event_index: u32) -> String {
    format!("{CODEX_SESSION_ID_PREFIX}:{thread_id}:{event_index}")
}

/// Only allowlisted accounting metadata is persisted. No request body, URL or headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CodexRequestRecord {
    pub id: String,
    pub origin: CodexUsageOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    pub at_ms: u64,
    pub billable: bool,
    pub profile_id: Option<String>,
    pub route_revision: Option<String>,
    pub upstream_protocol: Option<UpstreamProtocol>,
    pub request_model: Option<String>,
    pub mapped_model: Option<String>,
    pub response_model: Option<String>,
    pub status: Option<u16>,
    pub duration_ms: u64,
    pub first_byte_latency_ms: Option<u64>,
    pub first_token_latency_ms: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_creation_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub billing: CodexBilling,
    pub cost: Option<CodexRequestCost>,
    pub pricing_error: Option<String>,
    pub attempts: Vec<CodexRequestAttempt>,
}
impl CodexRequestRecord {
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self.origin {
            CodexUsageOrigin::Proxy => {
                if uuid::Uuid::parse_str(&self.id).is_err() || self.attempts.len() > 32 {
                    return Err("Codex 请求记录标识或尝试次数无效".into());
                }
            }
            CodexUsageOrigin::Session => {
                let rest = self
                    .id
                    .strip_prefix(CODEX_SESSION_ID_PREFIX)
                    .and_then(|rest| rest.strip_prefix(':'))
                    .ok_or_else(|| "Codex 会话记录标识前缀无效".to_string())?;
                let mut parts = rest.splitn(2, ':');
                let thread = parts.next().unwrap_or_default();
                let index = parts.next().unwrap_or_default();
                if uuid::Uuid::parse_str(thread).is_err()
                    || index.is_empty()
                    || !index.bytes().all(|byte| byte.is_ascii_digit())
                    || index.parse::<u32>().is_err()
                {
                    return Err("Codex 会话记录标识或事件序号无效".into());
                }
                // The id's thread segment is the trailing rollout UUID while
                // `thread_id` is the logical thread from the root meta; the two
                // legitimately differ for revert/resume replacement rollouts.
                if self
                    .thread_id
                    .as_deref()
                    .is_some_and(|id| uuid::Uuid::parse_str(id).is_err())
                {
                    return Err("Codex 会话记录线程标识无效".into());
                }
                if !self.attempts.is_empty()
                    || self.duration_ms != 0
                    || self.status != Some(200)
                    || self.profile_id.is_some()
                    || self.route_revision.is_some()
                    || self.upstream_protocol.is_some()
                    || self.request_model.is_some()
                    || self.response_model.is_some()
                    || self.first_byte_latency_ms.is_some()
                    || self.first_token_latency_ms.is_some()
                {
                    return Err("Codex 会话记录不得携带网关请求字段".into());
                }
                if self.input_tokens.is_none()
                    || self.output_tokens.is_none()
                    || self.cache_read_tokens.is_none()
                {
                    return Err("Codex 会话记录缺少 token 总量".into());
                }
            }
        }
        self.billing.validate()?;
        for value in [
            Some(self.at_ms),
            Some(self.duration_ms),
            self.first_byte_latency_ms,
            self.first_token_latency_ms,
            self.input_tokens,
            self.output_tokens,
            self.cache_read_tokens,
            self.cache_creation_tokens,
            self.reasoning_tokens,
        ] {
            if value.is_some_and(|value| value > i64::MAX as u64) {
                return Err("Codex 请求计数超出可存储范围".into());
            }
        }
        if self
            .status
            .is_some_and(|status| !(100..=599).contains(&status))
        {
            return Err("Codex 请求状态码无效".into());
        }
        for value in [
            &self.profile_id,
            &self.route_revision,
            &self.thread_id,
            &self.request_model,
            &self.mapped_model,
            &self.response_model,
            &self.pricing_error,
        ]
        .into_iter()
        .flatten()
        {
            if value.len() > 2048 || value.chars().any(|c| c.is_control()) {
                return Err("Codex 请求元数据过长或含有控制字符".into());
            }
        }
        if let Some(cost) = &self.cost {
            if asb_core::contracts::decimal_micros(&cost.total_usd)? > i64::MAX as u64 {
                return Err("Codex 请求费用超出可存储范围".into());
            }
        }
        Ok(())
    }
    pub(crate) fn cost_micros(&self) -> Result<Option<u64>, String> {
        self.cost
            .as_ref()
            .map(|cost| asb_core::contracts::decimal_micros(&cost.total_usd))
            .transpose()
    }
}
