//! Persisted gateway listener state: read, validate, and write the current
//! contract without interpreting or replacing older state files.

use super::*;

pub(super) const STATE_VERSION: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GatewayStateFile {
    pub(crate) version: u8,
    pub(crate) identity: String,
    pub(crate) port: u16,
    /// Codex has a separate route record because its client capability is
    /// installation-scoped while `revision` selects the active third-party
    /// provider. This is intentionally not the old app-keyed route map.
    pub(crate) codex_route: Option<PersistedRoute>,
    /// Claude remains an independent route record under the same listener.
    pub(crate) claude_route: Option<PersistedRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PersistedRoute {
    pub(crate) profile_id: String,
    pub(crate) revision: String,
}

impl GatewayStateFile {
    pub(crate) fn route(&self, app: AppKind) -> Option<&PersistedRoute> {
        match app {
            AppKind::Codex => self.codex_route.as_ref(),
            AppKind::Claude => self.claude_route.as_ref(),
        }
    }

    pub(crate) fn set_route(&mut self, app: AppKind, route: Option<PersistedRoute>) {
        match app {
            AppKind::Codex => self.codex_route = route,
            AppKind::Claude => self.claude_route = route,
        }
    }

    #[cfg(test)]
    #[allow(dead_code)] // verification harness
    pub(crate) fn has_routes(&self) -> bool {
        self.codex_route.is_some() || self.claude_route.is_some()
    }
}

/// The outcome of loading the persisted state. An existing file that does not
/// satisfy the current contract is left byte-for-byte untouched and makes the
/// listener unavailable until the user explicitly repairs the installation.
pub(super) enum StateLoad {
    Ready(GatewayStateFile),
    /// No state file existed; a fresh state was created and persisted.
    Created(GatewayStateFile),
    /// The state file exists but is not the current contract. Its contents are
    /// never replaced at startup. Explicit retry preserves a diagnostic copy
    /// before creating a new identity; it never imports the old routes.
    Unusable(String),
}

pub(super) fn read_or_create_state(path: &Path) -> StateLoad {
    match fs::read(path) {
        Ok(bytes) => match parse_state(&bytes) {
            Ok(state) => StateLoad::Ready(state),
            Err(detail) => StateLoad::Unusable(detail),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let state = fresh_state();
            match write_state(path, &state) {
                Ok(()) => StateLoad::Created(state),
                Err(error) => StateLoad::Unusable(error),
            }
        }
        Err(error) => StateLoad::Unusable(format!("本机协议网关状态不可读：{error}")),
    }
}

pub(super) fn parse_state(bytes: &[u8]) -> Result<GatewayStateFile, String> {
    let state: GatewayStateFile = serde_json::from_slice(bytes).map_err(|_| {
        "本机协议网关状态不是当前版本；请在网关页重试，保留诊断副本并重建状态后再重新应用供应商"
            .to_string()
    })?;
    if state.version != STATE_VERSION
        || state.identity.trim().is_empty()
        || !valid_persisted_port(state.port)
    {
        return Err("本机协议网关状态版本或内容无效；请在网关页重试，保留诊断副本并重建状态后再重新应用供应商".into());
    }
    Ok(state)
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

pub(super) fn fresh_state() -> GatewayStateFile {
    GatewayStateFile {
        version: STATE_VERSION,
        identity: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
        port: initial_port(),
        codex_route: None,
        claude_route: None,
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

pub(super) fn write_state(path: &Path, state: &GatewayStateFile) -> Result<(), String> {
    let content = serde_json::to_string_pretty(state)
        .map_err(|_| "无法序列化本机协议网关状态".to_string())?;
    crate::config_store::write_json_atomic(path, &content)
        .map_err(|error| format!("无法持久化本机协议网关状态：{error}"))
}
