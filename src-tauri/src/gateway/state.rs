//! Persisted gateway listener state: read, validate, quarantine, write.

use super::*;

pub(super) const STATE_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GatewayStateFile {
    pub(crate) version: u8,
    pub(crate) identity: String,
    pub(crate) port: u16,
    pub(crate) active: BTreeMap<AppKind, PersistedRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PersistedRoute {
    pub(crate) profile_id: String,
    pub(crate) fingerprint: String,
}

/// The outcome of loading the persisted state. A corrupted file is
/// quarantined for diagnosis but never silently replaced: the controller
/// receives [`StateLoad::Quarantined`] and enters the explicit repair flow
/// whenever a client still depends on the gateway.
pub(super) enum StateLoad {
    Ready(GatewayStateFile),
    /// No state file existed; a fresh state was created and persisted.
    Created(GatewayStateFile),
    /// The previous file was invalid and quarantined; the replacement was
    /// persisted. Identity and routes from the old file are unrecoverable.
    Quarantined(GatewayStateFile),
    /// The state file exists but cannot be replaced or read. No listener may
    /// start from an unknown identity.
    Unusable(String),
}

pub(super) fn read_or_create_state(path: &Path) -> StateLoad {
    match fs::read_to_string(path) {
        Ok(text) => {
            let state: GatewayStateFile = match serde_json::from_str(&text) {
                Ok(state) => state,
                Err(_) => return replace_invalid_state(path),
            };
            if state.version != STATE_VERSION
                || state.identity.trim().is_empty()
                || !valid_persisted_port(state.port)
            {
                return replace_invalid_state(path);
            }
            StateLoad::Ready(state)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let state = fresh_state();
            match write_state(path, &state) {
                Ok(()) => StateLoad::Created(state),
                Err(write_error) => StateLoad::Unusable(write_error),
            }
        }
        Err(error) => StateLoad::Unusable(format!("本机协议网关状态不可读：{error}")),
    }
}

fn valid_persisted_port(port: u16) -> bool {
    #[cfg(test)]
    {
        // Isolated tests use port 0 only before the first successful bind so
        // parallel cases can ask the OS for a socket. Production state never
        // accepts that sentinel.
        port == 0 || validate_custom_port(port).is_ok()
    }
    #[cfg(not(test))]
    {
        validate_custom_port(port).is_ok()
    }
}

fn replace_invalid_state(path: &Path) -> StateLoad {
    let state = fresh_state();
    let quarantined = quarantine_invalid_state(path);
    match write_state(path, &state) {
        Ok(()) if quarantined.is_ok() => StateLoad::Quarantined(state),
        Ok(()) => StateLoad::Unusable("本机协议网关状态格式无效，且无法保留诊断副本".to_string()),
        Err(write_error) => {
            let detail = match quarantined {
                Ok(()) => write_error,
                Err(quarantine_error) => format!("{quarantine_error}；{write_error}"),
            };
            StateLoad::Unusable(detail)
        }
    }
}

pub(super) fn fresh_state() -> GatewayStateFile {
    GatewayStateFile {
        version: STATE_VERSION,
        identity: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
        port: initial_port(),
        active: BTreeMap::new(),
    }
}

/// Production installs own one fixed product default so client endpoint
/// configurations stay stable; isolated tests keep the OS-assigned port `0`
/// so parallel suites never fight over the default.
#[cfg(test)]
fn initial_port() -> u16 {
    0
}

#[cfg(not(test))]
fn initial_port() -> u16 {
    super::DEFAULT_GATEWAY_PORT
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
    let content = serde_json::to_string_pretty(state)
        .map_err(|_| "无法序列化本机协议网关状态".to_string())?;
    crate::config_store::write_json_atomic(path, &content)
        .map_err(|error| format!("无法持久化本机协议网关状态：{error}"))
}
