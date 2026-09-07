//! Persisted gateway listener state: read, validate, quarantine, write.

use super::*;

pub(super) const STATE_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct GatewayStateFile {
    pub(super) version: u8,
    pub(super) identity: String,
    pub(super) port: u16,
    pub(super) active: BTreeMap<AppKind, PersistedRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PersistedRoute {
    pub(super) profile_id: String,
    pub(super) fingerprint: String,
}

pub(super) fn read_or_create_state(path: &Path) -> Result<GatewayStateFile, String> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let state: GatewayStateFile = match serde_json::from_str(&text) {
                Ok(state) => state,
                Err(_) => {
                    quarantine_invalid_state(path)?;
                    return Ok(fresh_state());
                }
            };
            if state.version != STATE_VERSION || state.identity.trim().is_empty() || state.port == 0
            {
                quarantine_invalid_state(path)?;
                return Ok(fresh_state());
            }
            Ok(state)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(fresh_state()),
        Err(_) => Err("本机协议网关状态不可读".to_string()),
    }
}

pub(super) fn fresh_state() -> GatewayStateFile {
    GatewayStateFile {
        version: STATE_VERSION,
        identity: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
        port: 0,
        active: BTreeMap::new(),
    }
}

/// Retains corrupt application-owned state for diagnosis instead of deleting
/// it. The replacement starts with no routes, so a stale client configuration
/// cannot inherit an unknown upstream route.
pub(super) fn quarantine_invalid_state(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "本机协议网关状态路径无效".to_string())?;
    let quarantined = parent.join(format!("gateway.invalid.{}.json", Uuid::new_v4().simple()));
    fs::rename(path, quarantined)
        .map_err(|_| "本机协议网关状态格式无效，且无法保留诊断副本".to_string())
}

pub(super) fn write_state(path: &Path, state: &GatewayStateFile) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "本机协议网关状态路径无效".to_string())?;
    fs::create_dir_all(parent).map_err(|_| "无法创建本机协议网关状态目录".to_string())?;
    let temporary = parent.join(format!("gateway.{}.tmp", Uuid::new_v4().simple()));
    let content = serde_json::to_string_pretty(state)
        .map_err(|_| "无法序列化本机协议网关状态".to_string())?;
    fs::write(&temporary, content).map_err(|_| "无法写入本机协议网关状态".to_string())?;
    if fs::rename(&temporary, path).is_err() {
        let _ = fs::remove_file(&temporary);
        return Err("无法原子保存本机协议网关状态".to_string());
    }
    Ok(())
}
