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

/// Only allowlisted accounting metadata is persisted. No request body, URL or headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CodexRequestRecord {
    pub id: String,
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
        if uuid::Uuid::parse_str(&self.id).is_err() || self.attempts.len() > 32 {
            return Err("Codex 请求记录标识或尝试次数无效".into());
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
