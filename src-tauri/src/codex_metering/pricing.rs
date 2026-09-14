use super::{CodexPricingModelSource, CodexRequestCost, CodexRequestRecord};
use asb_core::contracts::{decimal_micros, format_usd_micros};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CodexModelPrice {
    pub input_usd_per_million: String,
    pub output_usd_per_million: String,
    pub cache_read_usd_per_million: String,
    pub cache_creation_usd_per_million: String,
    pub source: String,
}
impl CodexModelPrice {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.source.trim().is_empty()
            || self.source.len() > 512
            || self.source.chars().any(char::is_control)
        {
            return Err("Codex 模型价格需要有效的来源说明".into());
        }
        for price in [
            &self.input_usd_per_million,
            &self.output_usd_per_million,
            &self.cache_read_usd_per_million,
            &self.cache_creation_usd_per_million,
        ] {
            decimal_micros(price)?;
        }
        Ok(())
    }
}

/// Input is the total including cached tokens. Reasoning is already part of output.
/// Missing prices or incomplete usage stay unknown, never an invented zero charge.
pub(crate) fn estimate(
    record: &CodexRequestRecord,
    prices: &BTreeMap<String, CodexModelPrice>,
) -> Result<Option<CodexRequestCost>, String> {
    record.billing.validate()?;
    let model = match record.billing.model_source {
        CodexPricingModelSource::Request => record.mapped_model.as_deref(),
        CodexPricingModelSource::Response => record
            .response_model
            .as_deref()
            .or(record.mapped_model.as_deref()),
    };
    let Some((model, price)) = model.and_then(|model| prices.get_key_value(model)) else {
        return Ok(None);
    };
    price.validate()?;
    let (Some(input), Some(output)) = (record.input_tokens, record.output_tokens) else {
        return Ok(None);
    };
    let read = record.cache_read_tokens.unwrap_or(0);
    let create = record.cache_creation_tokens.unwrap_or(0);
    let fresh = input
        .checked_sub(read)
        .and_then(|n| n.checked_sub(create))
        .ok_or("Codex 缓存用量超过输入总量，不能估算费用")?;
    let subtotal = [
        (fresh, &price.input_usd_per_million),
        (output, &price.output_usd_per_million),
        (read, &price.cache_read_usd_per_million),
        (create, &price.cache_creation_usd_per_million),
    ]
    .into_iter()
    .try_fold(0u128, |sum, (tokens, price)| {
        sum.checked_add(u128::from(tokens) * u128::from(decimal_micros(price)?))
            .ok_or_else(|| "Codex 费用计算溢出".to_string())
    })?;
    let micros = subtotal
        .checked_mul(u128::from(decimal_micros(&record.billing.cost_multiplier)?))
        .and_then(|n| n.checked_add(500_000_000_000))
        .map(|n| n / 1_000_000_000_000)
        .and_then(|n| u64::try_from(n).ok())
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or("Codex 费用计算溢出")?;
    Ok(Some(CodexRequestCost {
        model: model.clone(),
        source: price.source.clone(),
        multiplier: record.billing.cost_multiplier.clone(),
        total_usd: format_usd_micros(micros),
    }))
}
