//! Claude-only traffic policy. This never enters a Codex profile or client file.

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeTrafficSettings {
    #[serde(default = "restore_on_exit_default")]
    pub(crate) restore_on_exit: bool,
    pub(crate) headers_timeout_seconds: u32,
    pub(crate) streaming_first_byte_timeout_seconds: u32,
    pub(crate) streaming_idle_timeout_seconds: u32,
    pub(crate) non_streaming_timeout_seconds: u32,
    pub(crate) circuit_failure_threshold: u32,
    pub(crate) circuit_success_threshold: u32,
    pub(crate) circuit_cooldown_seconds: u32,
    pub(crate) circuit_error_rate_percent: u32,
    pub(crate) circuit_min_requests: u32,
}

impl Default for ClaudeTrafficSettings {
    fn default() -> Self {
        Self {
            restore_on_exit: true,
            headers_timeout_seconds: 20,
            streaming_first_byte_timeout_seconds: 60,
            streaming_idle_timeout_seconds: 120,
            non_streaming_timeout_seconds: 600,
            circuit_failure_threshold: 3,
            circuit_success_threshold: 1,
            circuit_cooldown_seconds: 30,
            circuit_error_rate_percent: 60,
            circuit_min_requests: 10,
        }
    }
}

impl ClaudeTrafficSettings {
    pub(crate) fn validate(&self) -> Result<(), String> {
        for (name, value, max) in [
            ("响应头超时", self.headers_timeout_seconds, 600),
            (
                "流式首包超时",
                self.streaming_first_byte_timeout_seconds,
                3600,
            ),
            ("流式空闲超时", self.streaming_idle_timeout_seconds, 3600),
            ("非流式总超时", self.non_streaming_timeout_seconds, 7200),
            ("熔断失败阈值", self.circuit_failure_threshold, 100),
            ("熔断恢复阈值", self.circuit_success_threshold, 100),
            ("熔断冷却时间", self.circuit_cooldown_seconds, 3600),
            ("熔断错误率百分比", self.circuit_error_rate_percent, 100),
            ("错误率最小请求数", self.circuit_min_requests, 10000),
        ] {
            if value == 0 || value > max {
                return Err(format!("Claude {name}必须在 1–{max} 之间"));
            }
        }
        Ok(())
    }

    pub(crate) fn health_config(&self) -> super::ProviderHealthConfig {
        super::ProviderHealthConfig {
            failure_threshold: self.circuit_failure_threshold,
            open_cooldown: Duration::from_secs(self.circuit_cooldown_seconds.into()),
            success_threshold: self.circuit_success_threshold,
            error_rate_percent: self.circuit_error_rate_percent,
            min_requests: self.circuit_min_requests,
        }
    }
}

fn restore_on_exit_default() -> bool {
    true
}
