//! Claude-only request billing and cache policy. Client preferences retain their own file.

use serde::{Deserialize, Serialize};
use super::money::decimal_micros;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ClaudePricingModelSource {
    Request,
    #[default]
    Response,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeBilling {
    pub cost_multiplier: String,
    pub model_source: ClaudePricingModelSource,
    pub daily_limit_usd: Option<String>,
    pub monthly_limit_usd: Option<String>,
}

impl Default for ClaudeBilling {
    fn default() -> Self {
        Self {
            cost_multiplier: "1".into(),
            model_source: Default::default(),
            daily_limit_usd: None,
            monthly_limit_usd: None,
        }
    }
}

impl ClaudeBilling {
    pub fn validate(&self) -> Result<(), String> {
        decimal_micros(&self.cost_multiplier)?;
        for value in [&self.daily_limit_usd, &self.monthly_limit_usd]
            .into_iter()
            .flatten()
        {
            if decimal_micros(value)? == 0 {
                return Err("Claude 消费限额必须大于零".into());
            }
        }
        Ok(())
    }
}

