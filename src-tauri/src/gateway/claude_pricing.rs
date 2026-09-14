//! Local-only Claude gateway prices. Unknown prices are never reported as free.

use super::request_ledger::{ClaudeRequestCost, ClaudeRequestRecord};
use asb_core::contracts::{
    decimal_micros, format_usd_micros, ClaudeBilling, ClaudePricingModelSource,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

const FILE: &str = "claude-pricing.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeModelPrice {
    pub(crate) input_usd_per_million: String,
    pub(crate) output_usd_per_million: String,
    pub(crate) cache_read_usd_per_million: String,
    pub(crate) cache_creation_usd_per_million: String,
    pub(crate) source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudePriceBook {
    pub(crate) version: u8,
    pub(crate) models: BTreeMap<String, ClaudeModelPrice>,
}

impl Default for ClaudePriceBook {
    fn default() -> Self {
        let mut models = BTreeMap::new();
        for (model, input, output, read, create) in [
            ("claude-opus-5", "5", "25", "0.5", "6.25"),
            ("claude-sonnet-5", "3", "15", "0.3", "3.75"),
            ("claude-haiku-4-5", "1", "5", "0.1", "1.25"),
        ] {
            models.insert(
                model.into(),
                ClaudeModelPrice {
                    input_usd_per_million: input.into(),
                    output_usd_per_million: output.into(),
                    cache_read_usd_per_million: read.into(),
                    cache_creation_usd_per_million: create.into(),
                    source: "CC Switch d695a2d / 本地参考价，非账单".into(),
                },
            );
        }
        Self { version: 1, models }
    }
}

impl ClaudePriceBook {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.models.len() > 5000 {
            return Err("Claude 价格表版本或数量无效".into());
        }
        for (model, price) in &self.models {
            if model.trim().is_empty()
                || model.len() > 512
                || model.chars().any(char::is_control)
                || price.source.trim().is_empty()
                || price.source.len() > 512
            {
                return Err("Claude 价格表模型或来源无效".into());
            }
            for value in [
                &price.input_usd_per_million,
                &price.output_usd_per_million,
                &price.cache_read_usd_per_million,
                &price.cache_creation_usd_per_million,
            ] {
                decimal_micros(value)?;
            }
        }
        Ok(())
    }

    pub(crate) fn load(root: &Path) -> Result<Self, String> {
        let Some(text) = crate::config_store::read_optional(&root.join(FILE))
            .map_err(|e| format!("Claude 价格表不可读：{e}"))?
        else {
            return Ok(Self::default());
        };
        let book: Self = crate::config_store::parse_strict(&text)
            .map_err(|e| format!("Claude 价格表无效：{e}"))?;
        book.validate()?;
        Ok(book)
    }

    pub(crate) fn save(&self, root: &Path) -> Result<(), String> {
        self.validate()?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        crate::config_store::write_json_atomic(&root.join(FILE), &text).map_err(|e| e.to_string())
    }

    pub(crate) fn estimate(
        &self,
        record: &ClaudeRequestRecord,
        billing: &ClaudeBilling,
    ) -> Result<Option<ClaudeRequestCost>, String> {
        billing.validate()?;
        let model = match billing.model_source {
            ClaudePricingModelSource::Request => record.mapped_model.as_deref(),
            ClaudePricingModelSource::Response => record
                .response_model
                .as_deref()
                .or(record.mapped_model.as_deref()),
        };
        let Some((model, price)) = model.and_then(|model| self.models.get_key_value(model)) else {
            return Ok(None);
        };
        let (Some(input), Some(output)) = (record.input_tokens, record.output_tokens) else {
            return Ok(None);
        };
        let read = record.cache_read_tokens.unwrap_or(0);
        let create = record.cache_creation_tokens.unwrap_or(0);
        let fresh = input
            .checked_sub(read)
            .and_then(|n| n.checked_sub(create))
            .ok_or_else(|| "Claude 缓存用量超过输入总量，无法计费".to_string())?;
        let subtotal = [
            (fresh, &price.input_usd_per_million),
            (output, &price.output_usd_per_million),
            (read, &price.cache_read_usd_per_million),
            (create, &price.cache_creation_usd_per_million),
        ]
        .into_iter()
        .try_fold(0u128, |sum, (tokens, rate)| {
            sum.checked_add(u128::from(tokens) * u128::from(decimal_micros(rate)?))
                .ok_or_else(|| "Claude 费用计算超出范围".to_string())
        })?;
        let micros = subtotal
            .checked_mul(u128::from(decimal_micros(&billing.cost_multiplier)?))
            .and_then(|n| n.checked_add(500_000_000_000))
            .map(|n| n / 1_000_000_000_000)
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| "Claude 费用计算超出范围".to_string())?;
        Ok(Some(ClaudeRequestCost {
            model: model.clone(),
            source: price.source.clone(),
            multiplier: billing.cost_multiplier.clone(),
            total_usd: format_usd_micros(micros),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_prices_never_replace_the_previous_book() {
        let dir = tempfile::tempdir().unwrap();
        let mut book = ClaudePriceBook::default();
        book.save(dir.path()).unwrap();
        book.models
            .values_mut()
            .next()
            .unwrap()
            .input_usd_per_million = "NaN".into();
        assert!(book.save(dir.path()).is_err());
        assert_eq!(
            ClaudePriceBook::load(dir.path()).unwrap(),
            ClaudePriceBook::default()
        );
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudePriceBookSnapshot {
    pub(crate) book: ClaudePriceBook,
    pub(crate) file_hash: String,
}

pub(crate) fn read_snapshot(root: &Path) -> Result<ClaudePriceBookSnapshot, String> {
    let text = crate::config_store::read_optional(&root.join(FILE)).map_err(|e| e.to_string())?;
    let book = match &text {
        Some(text) => crate::config_store::parse_strict(text).map_err(|e| e.to_string())?,
        None => ClaudePriceBook::default(),
    };
    book.validate()?;
    Ok(ClaudePriceBookSnapshot {
        book,
        file_hash: asb_switch::sha256_hex(text.as_deref().unwrap_or("")),
    })
}

#[cfg(test)]
mod quote_tests {
    use super::*;
    fn record() -> ClaudeRequestRecord {
        serde_json::from_value(serde_json::json!({"at":"2026-09-13T00:00:00Z", "profileId":"p", "routeRevision":"r", "clientProtocol":"anthropicMessages", "upstreamProtocol":"responses", "requestModel":"asb-claude-primary", "mappedModel":"vendor-model", "responseModel":"claude-sonnet-5", "inputTokens":1000000, "outputTokens":100000, "cacheReadTokens":500000, "cacheCreationTokens":100000, "reasoningTokens":50000, "status":200, "durationMs":20, "firstByteLatencyMs":1})).unwrap()
    }
    #[test]
    fn bills_fresh_cache_read_cache_create_once_and_does_not_double_bill_reasoning() {
        let billing = ClaudeBilling {
            cost_multiplier: "2".into(),
            ..Default::default()
        };
        let cost = ClaudePriceBook::default()
            .estimate(&record(), &billing)
            .unwrap()
            .unwrap();
        assert_eq!(cost.total_usd, "6.450000");
        assert_eq!(cost.model, "claude-sonnet-5");
    }
    #[test]
    fn request_model_pricing_and_unknown_models_remain_distinct_from_free() {
        let billing = ClaudeBilling {
            model_source: ClaudePricingModelSource::Request,
            ..Default::default()
        };
        assert!(ClaudePriceBook::default()
            .estimate(&record(), &billing)
            .unwrap()
            .is_none());
        let mut record = record();
        record.cache_read_tokens = Some(1000001);
        assert!(ClaudePriceBook::default()
            .estimate(&record, &ClaudeBilling::default())
            .is_err());
    }
}
