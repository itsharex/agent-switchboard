use crate::gateway::server::transport::Timeouts;
use crate::gateway::ProviderHealthConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

/// Codex upstream timing and circuit thresholds. Streaming requests are
/// bounded per phase (headers, first byte, idle gap, total); a non-streaming
/// request has no meaningful first-byte or idle phase, so its whole response
/// is bounded by one deadline instead. Zero means unlimited for every timeout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CodexTrafficSettings {
    pub headers_timeout_seconds: u32,
    pub first_byte_timeout_seconds: u32,
    pub idle_timeout_seconds: u32,
    pub total_timeout_seconds: u32,
    #[serde(default = "default_non_streaming_timeout_seconds")]
    pub non_streaming_timeout_seconds: u32,
    pub failure_threshold: u32,
    pub cooldown_seconds: u32,
    pub success_threshold: u32,
    pub error_rate_percent: u32,
    pub min_requests: u32,
}
impl Default for CodexTrafficSettings {
    fn default() -> Self {
        Self {
            headers_timeout_seconds: 20,
            first_byte_timeout_seconds: 30,
            idle_timeout_seconds: 60,
            total_timeout_seconds: 600,
            non_streaming_timeout_seconds: default_non_streaming_timeout_seconds(),
            failure_threshold: 3,
            cooldown_seconds: 30,
            success_threshold: 1,
            error_rate_percent: 60,
            min_requests: 10,
        }
    }
}
fn default_non_streaming_timeout_seconds() -> u32 {
    600
}
impl CodexTrafficSettings {
    /// Phase deadlines for one upstream attempt. Streaming keeps the four
    /// configured phases; a non-streaming body is one unit, so its first
    /// byte, idle gap and total all collapse onto the non-streaming deadline.
    pub(crate) fn timeouts(&self, stream: bool) -> Timeouts {
        let seconds = |value: u32| Duration::from_secs(value.into());
        if stream {
            return Timeouts {
                headers: seconds(self.headers_timeout_seconds),
                first_byte: seconds(self.first_byte_timeout_seconds),
                idle: seconds(self.idle_timeout_seconds),
                total: seconds(self.total_timeout_seconds),
            };
        }
        let whole = seconds(self.non_streaming_timeout_seconds);
        Timeouts {
            headers: seconds(self.headers_timeout_seconds),
            first_byte: whole,
            idle: whole,
            total: whole,
        }
    }
    pub(crate) fn health_config(&self) -> ProviderHealthConfig {
        ProviderHealthConfig {
            failure_threshold: self.failure_threshold,
            open_cooldown: Duration::from_secs(self.cooldown_seconds.into()),
            success_threshold: self.success_threshold,
            error_rate_percent: self.error_rate_percent,
            min_requests: self.min_requests,
        }
    }
    fn validate(&self) -> Result<(), String> {
        for value in [
            self.headers_timeout_seconds,
            self.first_byte_timeout_seconds,
            self.idle_timeout_seconds,
            self.total_timeout_seconds,
            self.non_streaming_timeout_seconds,
        ] {
            if value > 86_400 {
                return Err("Codex 请求超时不能大于 86400 秒；0 表示不限制".into());
            }
        }
        if !(1..=100).contains(&self.failure_threshold)
            || !(1..=100).contains(&self.success_threshold)
            || !(1..=86_400).contains(&self.cooldown_seconds)
            || !(1..=100).contains(&self.error_rate_percent)
            || !(1..=10_000).contains(&self.min_requests)
        {
            return Err("Codex 熔断阈值、冷却时间或错误率无效".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CodexGatewayPolicy {
    pub version: u8,
    pub takeover: bool,
    pub enabled: bool,
    pub provider_ids: Vec<String>,
    pub max_retries: u32,
    pub traffic: CodexTrafficSettings,
    /// Retry a failed attempt once with image blocks replaced by a text
    /// marker after an upstream modality rejection.
    #[serde(default = "default_true")]
    pub media_fallback: bool,
}

fn default_true() -> bool {
    true
}
impl Default for CodexGatewayPolicy {
    fn default() -> Self {
        Self {
            version: 1,
            takeover: false,
            enabled: false,
            provider_ids: Vec::new(),
            max_retries: 2,
            traffic: CodexTrafficSettings::default(),
            media_fallback: true,
        }
    }
}
impl CodexGatewayPolicy {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Codex 网关策略版本不受支持".into());
        }
        if self.max_retries > 10 || self.provider_ids.len() > 64 {
            return Err("Codex 故障转移最多重试 10 次、队列最多 64 项".into());
        }
        let mut ids = HashSet::new();
        for id in &self.provider_ids {
            if uuid::Uuid::parse_str(id).is_err() || !ids.insert(id) {
                return Err("Codex 故障转移队列包含无效或重复的供应商".into());
            }
        }
        if self.enabled && (!self.takeover || self.provider_ids.is_empty()) {
            return Err("启用 Codex 故障转移需要接管并指定有序供应商队列".into());
        }
        self.traffic.validate()
    }
    pub(crate) fn candidate_ids(&self, primary: &str) -> Vec<String> {
        let mut ids = vec![primary.to_string()];
        if self.enabled {
            ids.extend(
                self.provider_ids
                    .iter()
                    .filter(|id| id.as_str() != primary)
                    .cloned(),
            );
        }
        ids
    }
}

pub(crate) fn pending_path(root: &Path) -> std::path::PathBuf {
    root.join("codex/policy-switch.json")
}

pub(crate) fn path(root: &Path) -> std::path::PathBuf {
    root.join("codex/gateway-policy.json")
}

pub(crate) fn load(root: &Path) -> Result<(CodexGatewayPolicy, String), String> {
    let raw =
        crate::config_store::read_optional(&path(root)).map_err(|_| "Codex 网关策略不可读")?;
    let revision = asb_switch::sha256_hex(raw.as_deref().unwrap_or(""));
    let policy = match raw {
        Some(raw) => serde_json::from_str::<CodexGatewayPolicy>(&raw)
            .map_err(|_| "Codex 网关策略格式无效；原文件未更改")?,
        None => CodexGatewayPolicy::default(),
    };
    policy.validate()?;
    Ok((policy, revision))
}

pub(crate) fn save(root: &Path, policy: &CodexGatewayPolicy) -> Result<String, String> {
    policy.validate()?;
    let json = serde_json::to_string_pretty(policy).map_err(|_| "Codex 网关策略序列化失败")?;
    crate::config_store::write_json_atomic(&path(root), &json)?;
    Ok(asb_switch::sha256_hex(&json))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabling_failover_preserves_the_explicit_queue_and_restart_revision() {
        let directory = tempfile::tempdir().unwrap();
        let mut policy = CodexGatewayPolicy::default();
        policy.takeover = true;
        policy.enabled = true;
        policy.provider_ids = vec![
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        ];
        let revision = save(directory.path(), &policy).unwrap();
        assert_eq!(load(directory.path()).unwrap(), (policy.clone(), revision));
        policy.enabled = false;
        save(directory.path(), &policy).unwrap();
        assert_eq!(
            load(directory.path()).unwrap().0.provider_ids,
            policy.provider_ids
        );
        assert_eq!(
            policy.candidate_ids(&policy.provider_ids[0]),
            vec![policy.provider_ids[0].clone()]
        );
    }
    #[test]
    fn malformed_state_is_not_silently_reset_and_empty_reads_do_not_write() {
        let directory = tempfile::tempdir().unwrap();
        load(directory.path()).unwrap();
        assert!(!path(directory.path()).exists());
        std::fs::create_dir_all(path(directory.path()).parent().unwrap()).unwrap();
        std::fs::write(path(directory.path()), "broken-policy").unwrap();
        assert!(load(directory.path()).is_err());
        assert_eq!(
            std::fs::read_to_string(path(directory.path())).unwrap(),
            "broken-policy"
        );
        let mut policy = CodexGatewayPolicy::default();
        policy.enabled = true;
        assert!(policy.validate().is_err());
    }
    #[test]
    fn non_streaming_requests_use_one_whole_response_deadline() {
        let traffic = CodexTrafficSettings {
            headers_timeout_seconds: 7,
            first_byte_timeout_seconds: 11,
            idle_timeout_seconds: 13,
            total_timeout_seconds: 17,
            non_streaming_timeout_seconds: 900,
            ..CodexTrafficSettings::default()
        };
        let streaming = traffic.timeouts(true);
        assert_eq!(
            (
                streaming.headers,
                streaming.first_byte,
                streaming.idle,
                streaming.total
            ),
            (
                Duration::from_secs(7),
                Duration::from_secs(11),
                Duration::from_secs(13),
                Duration::from_secs(17)
            )
        );
        let whole = traffic.timeouts(false);
        assert_eq!(whole.headers, Duration::from_secs(7));
        assert!(
            [whole.first_byte, whole.idle, whole.total]
                .iter()
                .all(|phase| *phase == Duration::from_secs(900)),
            "非流式响应只受一个整体期限约束，不套用流式首包/空闲期限"
        );
        let unlimited = CodexTrafficSettings {
            non_streaming_timeout_seconds: 0,
            ..CodexTrafficSettings::default()
        };
        assert!(unlimited.timeouts(false).total.is_zero());
        assert!(unlimited.validate().is_ok());
        assert!(CodexTrafficSettings {
            non_streaming_timeout_seconds: 86_401,
            ..CodexTrafficSettings::default()
        }
        .validate()
        .is_err());
    }
    #[test]
    fn policies_saved_before_the_non_streaming_deadline_load_with_the_default() {
        let directory = tempfile::tempdir().unwrap();
        let mut saved = serde_json::to_value(CodexGatewayPolicy::default()).unwrap();
        saved["traffic"]
            .as_object_mut()
            .unwrap()
            .remove("nonStreamingTimeoutSeconds");
        std::fs::create_dir_all(path(directory.path()).parent().unwrap()).unwrap();
        std::fs::write(path(directory.path()), saved.to_string()).unwrap();
        let (policy, _) = load(directory.path()).unwrap();
        assert_eq!(policy.traffic.non_streaming_timeout_seconds, 600);
    }
}
