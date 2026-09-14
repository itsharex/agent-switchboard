//! Durable, credential-free Claude circuit state.
//!
//! The gateway route itself remains the source of truth for credentials and
//! endpoint details. This file only remembers enough health information to
//! avoid forgetting an open circuit across an application restart.

use super::provider_health::{ProviderHealthSnapshot, ProviderHealthState};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(crate) const FILE_NAME: &str = "claude-health.json";
const FILE_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HealthFile {
    version: u8,
    routes: BTreeMap<String, ProviderHealthSnapshot>,
}

/// The process-wide owner of the Claude health file.
#[derive(Debug)]
pub(crate) struct ClaudeHealthStore {
    path: PathBuf,
    routes: Mutex<BTreeMap<String, ProviderHealthSnapshot>>,
    writable: bool,
    warning: Option<String>,
}

impl ClaudeHealthStore {
    pub(crate) fn load(state_root: &Path) -> Result<Self, String> {
        Self::load_from_path(&state_root.join(FILE_NAME))
    }

    pub(crate) fn empty(state_root: &Path) -> Self {
        Self {
            path: state_root.join(FILE_NAME),
            routes: Mutex::new(BTreeMap::new()),
            writable: true,
            warning: None,
        }
    }

    pub(crate) fn load_from_path(path: &Path) -> Result<Self, String> {
        let text = match crate::config_store::read_optional(path) {
            Ok(Some(text)) => text,
            Ok(None) => {
                return Ok(Self {
                    path: path.to_path_buf(),
                    routes: Mutex::new(BTreeMap::new()),
                    writable: true,
                    warning: None,
                })
            }
            Err(_) => return Err("Claude 健康状态不可读".to_string()),
        };
        let file: HealthFile = serde_json::from_str(&text)
            .map_err(|_| "Claude 健康状态格式无效；请删除该状态文件后重启应用".to_string())?;
        validate_file(&file)?;
        Ok(Self {
            path: path.to_path_buf(),
            routes: Mutex::new(file.routes),
            writable: true,
            warning: None,
        })
    }

    pub(crate) fn load_preserving_damage(root: &Path) -> Self {
        match Self::load(root) {
            Ok(store) => store,
            Err(error) => {
                let mut store = Self::empty(root);
                let retained = retain_damage(&store.path);
                store.writable = retained.is_ok();
                store.warning = Some(match retained {
                    Ok(path) => format!("{error}；原始文件保留在 {}", path.display()),
                    Err(reason) => format!("{error}；未改写原文件：{reason}"),
                });
                store
            }
        }
    }

    pub(crate) fn warning(&self) -> Option<String> {
        self.warning.clone()
    }

    pub(crate) fn snapshot(&self, route_key: &str) -> Option<ProviderHealthSnapshot> {
        self.routes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(route_key)
            .cloned()
    }

    pub(crate) fn save_snapshot(
        &self,
        route_key: &str,
        snapshot: &ProviderHealthSnapshot,
    ) -> Result<(), String> {
        if !self.writable {
            return Err(self
                .warning
                .clone()
                .unwrap_or_else(|| "Claude 健康状态不可写".into()));
        }
        validate_route_key(route_key)?;
        validate_snapshot(snapshot)?;
        let mut routes = self
            .routes
            .lock()
            .map_err(|_| "Claude 健康状态锁不可用".to_string())?;
        let previous = routes.insert(route_key.to_string(), snapshot.clone());
        let file = HealthFile {
            version: FILE_VERSION,
            routes: routes.clone(),
        };
        if let Err(error) = write_file(&self.path, &file) {
            match previous {
                Some(previous) => {
                    routes.insert(route_key.to_string(), previous);
                }
                None => {
                    routes.remove(route_key);
                }
            }
            return Err(error);
        }
        Ok(())
    }
}

/// Hashes the route identity before it reaches disk. The endpoint and the
/// profile's routing fingerprint never appear in the persisted health file.
pub(crate) fn route_key(profile_id: &str, fingerprint: &str, base_url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"asb/claude-health/v1\0");
    for value in [profile_id, fingerprint, base_url] {
        hasher.update(value.len().to_string().as_bytes());
        hasher.update(b":");
        hasher.update(value.as_bytes());
        hasher.update(b"\0");
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate_file(file: &HealthFile) -> Result<(), String> {
    if file.version != FILE_VERSION {
        return Err("Claude 健康状态版本无效；请删除该状态文件后重启应用".to_string());
    }
    for (route_key, snapshot) in &file.routes {
        validate_route_key(route_key)?;
        validate_snapshot(snapshot)?;
    }
    Ok(())
}

fn validate_route_key(route_key: &str) -> Result<(), String> {
    if route_key.len() != 64 || !route_key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Claude 健康状态包含无效路由标识；请删除该状态文件后重启应用".to_string());
    }
    Ok(())
}

fn validate_snapshot(snapshot: &ProviderHealthSnapshot) -> Result<(), String> {
    if snapshot.failed_requests > snapshot.total_requests {
        return Err("Claude 健康状态计数无效".into());
    }
    match snapshot.state {
        ProviderHealthState::Open if snapshot.open_until_ms.is_none() => {
            Err("Claude 健康状态缺少熔断冷却截止时间；请删除该状态文件后重启应用".to_string())
        }
        ProviderHealthState::Closed | ProviderHealthState::HalfOpen
            if snapshot.open_until_ms.is_some() =>
        {
            Err("Claude 健康状态包含多余熔断冷却截止时间；请删除该状态文件后重启应用".to_string())
        }
        _ => Ok(()),
    }
}

fn write_file(path: &Path, file: &HealthFile) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(file).map_err(|_| "Claude 健康状态序列化失败".to_string())?;
    crate::config_store::write_json_atomic(path, &json)
        .map_err(|error| format!("无法持久化 Claude 健康状态：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn open_snapshot() -> ProviderHealthSnapshot {
        ProviderHealthSnapshot {
            state: ProviderHealthState::Open,
            consecutive_failures: 3,
            open_until_ms: Some(2_000),
            ..Default::default()
        }
    }

    #[test]
    fn route_key_is_stable_and_does_not_expose_route_material() {
        let key = route_key("profile", "fingerprint", "https://secret.example/api");
        assert_eq!(key.len(), 64);
        assert!(key.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(!key.contains("secret"));
        assert_eq!(
            key,
            route_key("profile", "fingerprint", "https://secret.example/api")
        );
        assert_ne!(
            key,
            route_key("profile", "other", "https://secret.example/api")
        );
    }

    #[test]
    fn missing_file_is_a_clean_empty_store() {
        let directory = tempdir().expect("temporary directory");
        let store = ClaudeHealthStore::load(directory.path()).expect("missing file is valid");
        assert!(store.snapshot(&route_key("p", "r", "u")).is_none());
        assert!(!directory.path().join(FILE_NAME).exists());
    }

    #[test]
    fn save_and_reload_uses_a_strict_credential_free_contract() {
        let directory = tempdir().expect("temporary directory");
        let store = ClaudeHealthStore::load(directory.path()).expect("load store");
        let key = route_key("profile", "fingerprint", "https://secret.example");
        store
            .save_snapshot(&key, &open_snapshot())
            .expect("save snapshot");

        let raw = fs::read_to_string(directory.path().join(FILE_NAME)).expect("health file");
        assert!(raw.contains("\"openUntilMs\""));
        assert!(!raw.contains("secret.example"));
        assert_eq!(
            ClaudeHealthStore::load(directory.path())
                .expect("reload store")
                .snapshot(&key),
            Some(open_snapshot())
        );
    }

    #[test]
    fn invalid_file_is_rejected_without_rewriting_original_bytes() {
        let directory = tempdir().expect("temporary directory");
        let path = directory.path().join(FILE_NAME);
        let original = br#"{"version":2,"routes":{}}"#;
        fs::create_dir_all(path.parent().expect("state parent")).expect("state dir");
        fs::write(&path, original).expect("invalid health file");

        let error = ClaudeHealthStore::load(directory.path()).expect_err("invalid file");

        assert!(error.contains("版本无效"));
        assert_eq!(fs::read(&path).expect("read original"), original);
    }

    #[test]
    fn invalid_snapshot_shape_is_rejected() {
        let directory = tempdir().expect("temporary directory");
        let path = directory.path().join(FILE_NAME);
        fs::write(
            &path,
            format!(
                r#"{{"version":1,"routes":{{"{}":{{"state":"closed","consecutiveFailures":0,"openUntilMs":1}}}}}}"#,
                "a".repeat(64)
            ),
        )
        .expect("invalid snapshot");

        assert!(ClaudeHealthStore::load(directory.path())
            .expect_err("invalid snapshot")
            .contains("多余熔断"));
    }
}

fn retain_damage(path: &Path) -> Result<PathBuf, String> {
    use std::io::Write;
    if !path.is_file() {
        return Err("健康状态路径不是可读取的文件".into());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let backup = path.with_file_name(format!(
        "claude-health.invalid-{}.json",
        uuid::Uuid::new_v4()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(backup)
}
