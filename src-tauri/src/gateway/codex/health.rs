//! Codex circuit persistence and reset generations; no Claude state is read.
use crate::gateway::provider_health::ProviderHealthSnapshot;
use crate::gateway::{ActiveRoute, ProviderHealth, ProviderHealthConfig, ProviderHealthState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HealthFile {
    version: u8,
    routes: BTreeMap<String, ProviderHealthSnapshot>,
}
struct RuntimeState {
    file: Result<HealthFile, String>,
    epochs: BTreeMap<String, u64>,
    trackers: BTreeMap<String, Arc<ProviderHealth>>,
}
pub(crate) struct CodexHealthStore {
    path: PathBuf,
    state: Mutex<RuntimeState>,
    error: Mutex<Option<String>>,
}

impl CodexHealthStore {
    pub(crate) fn new(root: &Path) -> Self {
        let path = root.join("codex/health.json");
        let file = read(&path);
        Self {
            path,
            error: Mutex::new(file.as_ref().err().cloned()),
            state: Mutex::new(RuntimeState {
                file,
                epochs: BTreeMap::new(),
                trackers: BTreeMap::new(),
            }),
        }
    }
    pub(crate) fn ensure_ready(&self) -> Result<(), String> {
        self.state
            .lock()
            .map_err(|_| "Codex 熔断状态锁不可用")?
            .file
            .as_ref()
            .map(|_| ())
            .map_err(Clone::clone)
    }
    pub(crate) fn warning(&self) -> Option<String> {
        self.error.lock().ok().and_then(|error| error.clone())
    }
    pub(crate) fn health_for(
        self: &Arc<Self>,
        route: &ActiveRoute,
        config: ProviderHealthConfig,
    ) -> Arc<ProviderHealth> {
        let key = route_key(route);
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(health) = state
            .trackers
            .get(&key)
            .filter(|health| health.config() == config)
        {
            return Arc::clone(health);
        }
        let epoch = state
            .epochs
            .get(&key)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        state.epochs.insert(key.clone(), epoch);
        let snapshot = state
            .file
            .as_ref()
            .ok()
            .and_then(|file| file.routes.get(&key).cloned());
        let store = Arc::downgrade(self);
        let persistence_key = key.clone();
        let persist = Arc::new(move |snapshot| {
            if let Some(store) = store.upgrade() {
                if let Err(error) = store.persist(&persistence_key, epoch, snapshot) {
                    log::warn!("无法保存 Codex 熔断状态：{error}");
                    if let Ok(mut warning) = store.error.lock() {
                        *warning = Some(error);
                    }
                }
            }
        });
        let health = Arc::new(ProviderHealth::with_snapshot_and_persistence(
            config,
            snapshot,
            Some(persist),
        ));
        state.trackers.insert(key, health.clone());
        health
    }
    fn persist(
        &self,
        key: &str,
        epoch: u64,
        snapshot: ProviderHealthSnapshot,
    ) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| "Codex 熔断状态锁不可用")?;
        if state.epochs.get(key).copied() != Some(epoch) {
            return Ok(());
        }
        let file = state.file.as_mut().map_err(|error| error.clone())?;
        let before = file.clone();
        file.routes.insert(key.to_string(), snapshot);
        if let Err(error) = write(&self.path, file) {
            *file = before;
            return Err(error);
        }
        Ok(())
    }
    pub(crate) fn reset(&self, route: &ActiveRoute) -> Result<(), String> {
        let key = route_key(route);
        let mut state = self.state.lock().map_err(|_| "Codex 熔断状态锁不可用")?;
        let file = state.file.as_mut().map_err(|error| error.clone())?;
        let before = file.clone();
        file.routes.remove(&key);
        if let Err(error) = write(&self.path, file) {
            *file = before;
            return Err(error);
        }
        let epoch = state
            .epochs
            .get(&key)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        state.epochs.insert(key.clone(), epoch);
        state.trackers.remove(&key);
        if let Ok(mut error) = self.error.lock() {
            *error = None;
        }
        Ok(())
    }
}

fn route_key(route: &ActiveRoute) -> String {
    let identity = serde_json::json!([
        "codex-health/v1",
        route.profile_id,
        route.fingerprint,
        route.upstream_base_url
    ]);
    asb_switch::sha256_hex(&identity.to_string())
}
fn read(path: &Path) -> Result<HealthFile, String> {
    let raw = crate::config_store::read_optional(path).map_err(|_| "Codex 熔断状态不可读")?;
    let file = match raw {
        Some(raw) => serde_json::from_str::<HealthFile>(&raw)
            .map_err(|_| "Codex 熔断状态损坏；原文件未更改")?,
        None => HealthFile {
            version: 1,
            routes: BTreeMap::new(),
        },
    };
    if file.version != 1 || file.routes.len() > 10_000 {
        return Err("Codex 熔断状态版本或数量无效".into());
    }
    for (key, snapshot) in &file.routes {
        if key.len() != 64
            || !key.bytes().all(|b| b.is_ascii_hexdigit())
            || (snapshot.state == ProviderHealthState::Open) != snapshot.open_until_ms.is_some()
        {
            return Err("Codex 熔断状态含有无效路由或冷却时间".into());
        }
    }
    Ok(file)
}
fn write(path: &Path, file: &HealthFile) -> Result<(), String> {
    let json = serde_json::to_string_pretty(file).map_err(|_| "Codex 熔断状态序列化失败")?;
    crate::config_store::write_json_atomic(path, &json)
}
