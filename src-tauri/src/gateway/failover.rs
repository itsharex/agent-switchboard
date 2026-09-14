//! Claude request failover policy.
//!
//! The policy is deliberately separate from provider storage and gateway
//! commands. Provider validation belongs to the caller; this module owns only
//! the ordered id list and its small persisted contract.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The file is rooted directly under the application's state directory.
pub(crate) const FILE_NAME: &str = "claude-failover.json";

/// Matches the reference client's user-facing retry range (0 through 10).
pub(crate) const MAX_RETRIES: u32 = 10;

/// Persisted Claude failover settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeFailoverPolicy {
    pub(crate) enabled: bool,
    pub(crate) provider_ids: Vec<String>,
    pub(crate) max_retries: u32,
    #[serde(default)]
    pub(crate) takeover: bool,
    #[serde(default)]
    pub(crate) traffic: super::claude_settings::ClaudeTrafficSettings,
}

impl Default for ClaudeFailoverPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            provider_ids: Vec::new(),
            max_retries: 0,
            takeover: false,
            traffic: Default::default(),
        }
    }
}

impl ClaudeFailoverPolicy {
    /// Canonicalizes queue ids and bounds the persisted retry setting.
    pub(crate) fn normalize(&mut self) {
        self.provider_ids = normalize_provider_ids(std::mem::take(&mut self.provider_ids));
        self.max_retries = self.max_retries.min(MAX_RETRIES);
    }

    pub(crate) fn normalized(mut self) -> Self {
        self.normalize();
        self
    }

    /// Returns the configured retry count after applying the policy bound.
    pub(crate) fn effective_retry_count(&self) -> u32 {
        self.max_retries.min(MAX_RETRIES)
    }

    /// maxRetries counts failures after the first request, so attempts are
    /// always one greater than the effective retry count.
    pub(crate) fn effective_attempts(&self) -> usize {
        self.effective_retry_count() as usize + 1
    }

    /// Orders caller-approved ids by the persisted queue priority.
    ///
    /// available_provider_ids must already contain only Claude custom
    /// providers. This method only intersects that set with the configured
    /// queue, preserving queue order and removing duplicates.
    pub(crate) fn ordered_provider_ids<I, S>(&self, available_provider_ids: I) -> Vec<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let available = normalize_provider_ids(available_provider_ids);
        let available_set: HashSet<String> = available.iter().cloned().collect();
        normalize_provider_ids(self.provider_ids.iter())
            .into_iter()
            .filter(|id| available_set.contains(id))
            .collect()
    }

    /// Builds the request order for one active provider. The active provider
    /// is always first, even when the persisted queue was edited externally
    /// and no longer contains it.
    pub(crate) fn candidate_provider_ids(
        &self,
        selected_provider_id: &str,
        available_provider_ids: &[String],
    ) -> Vec<String> {
        if !self.enabled {
            return vec![selected_provider_id.to_string()];
        }
        let mut ids = self.ordered_provider_ids(available_provider_ids.iter());
        if let Some(position) = ids.iter().position(|id| id == selected_provider_id) {
            ids.rotate_left(position);
        } else {
            ids.insert(0, selected_provider_id.to_string());
        }
        ids
    }
}

/// Trims ids, drops empty entries, and removes duplicates without changing
/// the explicit queue priority supplied by the caller.
pub(crate) fn normalize_provider_ids<I, S>(provider_ids: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut seen = HashSet::new();
    provider_ids
        .into_iter()
        .filter_map(|provider_id| {
            let provider_id = provider_id.as_ref().trim();
            (!provider_id.is_empty()).then(|| provider_id.to_string())
        })
        .filter(|provider_id| seen.insert(provider_id.clone()))
        .collect()
}

pub(crate) fn path(state_root: &Path) -> PathBuf {
    state_root.join(FILE_NAME)
}

/// Loads the policy from state_root/claude-failover.json.
///
/// A missing file is an intentional compatibility state, not an error.
pub(crate) fn load(state_root: &Path) -> Result<ClaudeFailoverPolicy, String> {
    load_from_path(&path(state_root))
}

pub(crate) fn load_from_path(path: &Path) -> Result<ClaudeFailoverPolicy, String> {
    let text = match crate::config_store::read_optional(path) {
        Ok(Some(text)) => text,
        Ok(None) => return Ok(ClaudeFailoverPolicy::default()),
        Err(_) => return Err("Claude 故障转移策略不可读".to_string()),
    };
    let policy = crate::config_store::parse_strict::<ClaudeFailoverPolicy>(&text)
        .map_err(|_| "Claude 故障转移策略格式无效".to_string())?;
    policy.traffic.validate()?;
    Ok(policy.normalized())
}

/// Persists a normalized policy through the shared atomic JSON writer.
pub(crate) fn save(state_root: &Path, policy: &ClaudeFailoverPolicy) -> Result<(), String> {
    save_to_path(&path(state_root), policy)
}

pub(crate) fn save_to_path(path: &Path, policy: &ClaudeFailoverPolicy) -> Result<(), String> {
    policy.traffic.validate()?;
    let json = serde_json::to_string_pretty(&policy.clone().normalized())
        .map_err(|_| "Claude 故障转移策略序列化失败".to_string())?;
    crate::config_store::write_json_atomic(path, &json)
        .map_err(|error| format!("无法持久化 Claude 故障转移策略：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn missing_file_returns_compatibility_default() {
        let directory = tempdir().expect("temporary directory");

        let policy = load(directory.path()).expect("missing policy is compatible");

        assert_eq!(policy, ClaudeFailoverPolicy::default());
        assert!(!path(directory.path()).exists());
    }

    #[test]
    fn read_write_uses_camel_case_and_atomic_state_path() {
        let directory = tempdir().expect("temporary directory");
        let input = ClaudeFailoverPolicy {
            enabled: true,
            provider_ids: vec![" p1 ".to_string(), "p1".to_string(), "p2".to_string()],
            max_retries: 4,
            ..Default::default()
        };

        save(directory.path(), &input).expect("save policy");

        let raw = fs::read_to_string(path(directory.path())).expect("policy file");
        assert!(raw.contains("\"providerIds\""));
        assert!(raw.contains("\"maxRetries\": 4"));
        assert!(!raw.contains("provider_ids"));
        assert_eq!(
            load(directory.path()).expect("load policy"),
            ClaudeFailoverPolicy {
                enabled: true,
                provider_ids: vec!["p1".to_string(), "p2".to_string()],
                max_retries: 4,
                ..Default::default()
            }
        );
    }

    #[test]
    fn unknown_fields_are_rejected_without_compatibility_fallback() {
        let directory = tempdir().expect("temporary directory");
        let policy_path = path(directory.path());
        fs::create_dir_all(policy_path.parent().expect("policy parent")).expect("state dir");
        fs::write(
            &policy_path,
            r#"{"enabled":false,"providerIds":[],"maxRetries":0,"legacy":true}"#,
        )
        .expect("malformed policy");

        assert_eq!(
            load(directory.path()).expect_err("unknown field must fail"),
            "Claude 故障转移策略格式无效"
        );
    }

    #[test]
    fn queue_normalization_preserves_priority_and_deduplicates() {
        let ids = normalize_provider_ids([" p2 ", "p1", "p2", "", "  ", "p1", "p3"]);

        assert_eq!(ids, vec!["p2", "p1", "p3"]);
    }

    #[test]
    fn ordered_candidates_follow_queue_priority() {
        let policy = ClaudeFailoverPolicy {
            enabled: true,
            provider_ids: vec!["p2".to_string(), "p1".to_string(), "stale".to_string()],
            max_retries: 2,
            ..Default::default()
        };

        assert_eq!(
            policy.ordered_provider_ids(["p1", "p3", "p2", "p1"]),
            vec!["p2", "p1"]
        );
    }

    #[test]
    fn max_retries_is_capped_and_attempts_are_retry_plus_one() {
        let policy = ClaudeFailoverPolicy {
            enabled: true,
            provider_ids: Vec::new(),
            max_retries: u32::MAX,
            ..Default::default()
        };

        assert_eq!(policy.effective_retry_count(), MAX_RETRIES);
        assert_eq!(policy.effective_attempts(), MAX_RETRIES as usize + 1);

        let disabled = ClaudeFailoverPolicy {
            enabled: false,
            ..policy
        };
        assert_eq!(disabled.effective_attempts(), MAX_RETRIES as usize + 1);
    }

    #[test]
    fn candidate_order_starts_with_selected_provider_even_when_queue_omits_it() {
        let policy = ClaudeFailoverPolicy {
            enabled: true,
            provider_ids: vec!["p2".to_string(), "p3".to_string()],
            max_retries: 2,
            ..Default::default()
        };
        let available = vec!["p1".to_string(), "p2".to_string(), "p3".to_string()];

        assert_eq!(
            policy.candidate_provider_ids("p1", &available),
            vec!["p1", "p2", "p3"]
        );
        assert_eq!(
            policy.candidate_provider_ids("p3", &available),
            vec!["p3", "p2"]
        );
    }

    #[test]
    fn disabled_policy_never_adds_standby_providers() {
        let policy = ClaudeFailoverPolicy::default();
        let available = vec!["p1".to_string(), "p2".to_string()];

        assert_eq!(policy.candidate_provider_ids("p1", &available), vec!["p1"]);
    }

    #[test]
    fn enabled_empty_queue_keeps_the_selected_provider_only() {
        let policy = ClaudeFailoverPolicy {
            enabled: true,
            provider_ids: Vec::new(),
            max_retries: 3,
            ..Default::default()
        };
        let available = vec!["p1".to_string(), "p2".to_string()];

        assert_eq!(policy.candidate_provider_ids("p2", &available), vec!["p2"]);
    }
}
