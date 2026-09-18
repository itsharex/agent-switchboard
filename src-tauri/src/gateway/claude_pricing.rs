//! Local-only Claude gateway prices. Unknown prices are never reported as free.

use super::claude_pricing_seed::BUILTIN_SEED;
use super::request_ledger::{ClaudeRequestCost, ClaudeRequestRecord};
use asb_core::contracts::{
    decimal_micros, format_usd_micros, ClaudeBilling, ClaudePricingModelSource,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

const FILE: &str = "claude-pricing.json";
const SOURCE: &str = "本地参考价，非账单";
/// The seed generation a file was last filled from. Version 1 files predate
/// the full vendor table and are filled on load; version 2 files are the
/// user's own state and load exactly as stored.
const SEED_VERSION: u8 = 2;
/// The three reference rows version 1 files were seeded with, in their old
/// values. A stored row equal to one of these was never customized, so the
/// fill refreshes it to the current seed (official 2026-09 list prices).
const V1_SEED: &[(&str, &str, &str, &str, &str)] = &[
    ("claude-opus-5", "5", "25", "0.5", "6.25"),
    ("claude-sonnet-5", "3", "15", "0.3", "3.75"),
    ("claude-haiku-4-5", "1", "5", "0.1", "1.25"),
];

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

fn seeded_price(input: &str, output: &str, read: &str, create: &str) -> ClaudeModelPrice {
    ClaudeModelPrice {
        input_usd_per_million: input.into(),
        output_usd_per_million: output.into(),
        cache_read_usd_per_million: read.into(),
        cache_creation_usd_per_million: create.into(),
        source: SOURCE.into(),
    }
}

impl Default for ClaudePriceBook {
    fn default() -> Self {
        let mut models = BTreeMap::new();
        for (model, input, output, read, create) in BUILTIN_SEED {
            models.insert((*model).into(), seeded_price(input, output, read, create));
        }
        Self {
            version: SEED_VERSION,
            models,
        }
    }
}

impl ClaudePriceBook {
    /// Fills a book from an older seed generation: every built-in row the file
    /// lacks is added, and a version-1 row that still equals its old default
    /// is refreshed to the current seed. Rows the user touched stay theirs.
    fn filled_from_older_seed(mut self) -> Self {
        if self.version >= SEED_VERSION {
            return self;
        }
        for (model, input, output, read, create) in BUILTIN_SEED {
            match self.models.get_mut(*model) {
                Some(price) => {
                    let untouched = V1_SEED.iter().any(|(old, i, o, r, c)| {
                        *old == *model
                            && price.input_usd_per_million == *i
                            && price.output_usd_per_million == *o
                            && price.cache_read_usd_per_million == *r
                            && price.cache_creation_usd_per_million == *c
                            && price.source == SOURCE
                    });
                    if untouched {
                        *price = seeded_price(input, output, read, create);
                    }
                }
                None => {
                    self.models
                        .insert((*model).into(), seeded_price(input, output, read, create));
                }
            }
        }
        self.version = SEED_VERSION;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.version != SEED_VERSION || self.models.len() > 5000 {
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
        let book = book.filled_from_older_seed();
        book.validate()?;
        Ok(book)
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
        // 0.4×$2 + 0.1×$10 + 0.5×$0.20 + 0.1×$2.50 = $2.15, doubled by the
        // profile multiplier.
        assert_eq!(cost.total_usd, "4.300000");
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
    #[test]
    fn seed_covers_every_vendor_family_the_presets_and_gateway_route_to() {
        let book = ClaudePriceBook::default();
        for model in [
            "claude-opus-4-6-20260206",
            "claude-haiku-4-5-20251001",
            "gemini-3.6-flash",
            "gpt-5.6",
            "kimi-k2.6",
            "deepseek-v3",
            "grok-4.6",
            "qwen3.8-max",
        ] {
            assert!(
                book.models.contains_key(model),
                "{model} must carry a built-in reference price"
            );
        }
    }
    #[test]
    fn version_one_files_gain_the_full_seed_without_losing_customizations() {
        let mut stale = ClaudePriceBook {
            version: 1,
            models: BTreeMap::new(),
        };
        for (model, input, output, read, create) in V1_SEED {
            stale
                .models
                .insert((*model).into(), seeded_price(input, output, read, create));
        }
        // A user-customized price keeps both its values and its source.
        stale.models.insert(
            "claude-opus-5".into(),
            ClaudeModelPrice {
                input_usd_per_million: "9".into(),
                output_usd_per_million: "40".into(),
                cache_read_usd_per_million: "0.9".into(),
                cache_creation_usd_per_million: "5".into(),
                source: "user".into(),
            },
        );
        // A user addition survives untouched.
        stale.models.insert(
            "my-relay-model".into(),
            ClaudeModelPrice {
                input_usd_per_million: "1".into(),
                output_usd_per_million: "2".into(),
                cache_read_usd_per_million: "0".into(),
                cache_creation_usd_per_million: "0".into(),
                source: "user".into(),
            },
        );
        let filled = stale.filled_from_older_seed();
        assert_eq!(filled.version, SEED_VERSION);
        filled.validate().unwrap();
        // The stale seeded sonnet row was refreshed to the official list price.
        assert_eq!(
            filled.models["claude-sonnet-5"].output_usd_per_million,
            "10"
        );
        // The customized row kept the user's values.
        assert_eq!(filled.models["claude-opus-5"].input_usd_per_million, "9");
        // The user addition stayed.
        assert!(filled.models.contains_key("my-relay-model"));
        // The full vendor table arrived.
        assert!(filled.models.contains_key("gemini-3.6-flash"));
        // A version-2 file loads exactly as stored: deletions are honored.
        let mut trimmed = ClaudePriceBook::default();
        trimmed.models.remove("gemini-3.6-flash");
        assert_eq!(
            trimmed.clone().filled_from_older_seed(),
            trimmed,
            "current-generation files must never be re-filled"
        );
    }
}
