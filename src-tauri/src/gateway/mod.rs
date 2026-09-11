//! The private loopback gateway for every third-party Codex route and
//! cross-protocol Claude routes, including Responses transport and compaction.
//!
//! A provider profile remains the sole owner of its endpoint and API key. The
//! persisted gateway state stores only a listener identity plus a profile id
//! and fingerprint, so a restart never duplicates an upstream credential.
//!
//! The controller always exists once the application is running; the listener
//! behind it may be listening, failed, or awaiting repair. A failed bind is a
//! visible, recoverable runtime state — never a reason to refuse the window.

mod metrics;
pub(crate) mod port_change;
mod port_probe;
mod server;
mod transform;

use metrics::{GatewayMetrics, GatewayMetricsSnapshot};

use crate::local_state::LocalState;
use asb_core::adapter;
use asb_core::contracts::{
    AppKind, CodexRouteSnapshot, ProviderProfile, ResponsesOptions, RouteMode, SwitchPlan,
    UpstreamProtocol,
};
use asb_core::validate_plan;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;
#[cfg(test)]
use tiny_http::Server;
use uuid::Uuid;

mod activation_snapshot;
mod controller;
pub(crate) use activation_snapshot::GatewayActivationSnapshot;
mod compaction;
mod content_encoding;
pub(crate) mod http;
mod identity;
mod lifecycle;
mod restore_validation;
mod routing;
mod state;

#[cfg(test)]
mod responses_tests;
#[cfg(test)]
mod tests;

use identity::*;
use state::*;

pub(crate) use port_change::{
    GatewayPortChangePlan, GatewayPortChangeResult, PortChangePreparations,
};

/// The product default listener port for new installations. It is not
/// reserved by any system; the actual binding result stays the only truth.
/// Existing installations keep their persisted port across upgrades.
pub(crate) const DEFAULT_GATEWAY_PORT: u16 = 47821;

/// Custom listener ports must be explicit user choices in the unprivileged
/// registered range; port `0` is never accepted as user input.
pub(crate) const MIN_CUSTOM_PORT: u16 = 1024;

pub(crate) const MAX_CUSTOM_PORT: u16 = 65535;

pub(crate) const CUSTOM_PORT_RANGE_MESSAGE: &str = "监听端口必须是 1024–65535 之间的整数";

pub(crate) fn validate_custom_port(port: u16) -> Result<(), String> {
    if !(MIN_CUSTOM_PORT..=MAX_CUSTOM_PORT).contains(&port) {
        return Err(CUSTOM_PORT_RANGE_MESSAGE.to_string());
    }
    Ok(())
}

/// How long a port change waits for in-flight requests before cancelling.
pub(crate) const PORT_CHANGE_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

/// The result of resolving a selected provider into the exact client-facing
/// plan. Direct plans retain the provider endpoint; routed plans replace it
/// with one local, per-profile capability token.
#[derive(Clone)]
pub(crate) struct GatewayProjection {
    pub plan: SwitchPlan,
    activation: GatewayActivation,
    warning: Option<String>,
    pub(crate) codex_catalog: Option<CodexCatalogProjection>,
}

/// One immutable model-catalog file named from the selected provider and its
/// route revision. A restored configuration can therefore continue to point
/// at the exact catalog it was written with.
#[derive(Clone)]
pub(crate) struct CodexCatalogProjection {
    pub(crate) file_name: String,
    pub(crate) content: String,
}

impl GatewayProjection {
    pub(crate) fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }
}

/// A state transition that is committed only from the switch executor's
/// callback, after the client configuration has been atomically replaced.
#[derive(Clone)]
enum GatewayActivation {
    Direct { app: AppKind },
    Routed(ActiveRoute),
}

impl GatewayActivation {
    fn app(&self) -> AppKind {
        match self {
            Self::Direct { app } => *app,
            Self::Routed(route) => route.app,
        }
    }
}

/// A private in-memory route. Deliberately no `Debug` implementation: the
/// profile owns an API key and accidental diagnostics must not reveal it.
#[derive(Clone)]
pub(crate) struct ActiveRoute {
    pub(crate) app: AppKind,
    pub(crate) profile_id: String,
    pub(crate) fingerprint: String,
    pub(crate) client_token: String,
    pub(crate) continuation_key: [u8; 32],
    pub(crate) upstream_base_url: String,
    pub(crate) upstream_protocol: UpstreamProtocol,
    pub(crate) responses_options: Option<ResponsesOptions>,
    pub(crate) max_output_tokens: Option<u64>,
    pub(crate) api_key: String,
    /// Present only for Codex. This is the accepted-request routing source
    /// for its typed catalog, model mapping, and operation capabilities.
    pub(crate) codex: Option<CodexRouteSnapshot>,
}

/// One process-owned loopback listener. Its stop signal belongs to this
/// socket alone so a port handoff cannot accidentally stop the replacement.
#[derive(Clone)]
pub(crate) struct BoundListener {
    socket: Arc<Mutex<Option<std::net::TcpListener>>>,
    stop: Arc<AtomicBool>,
    port: u16,
}
impl BoundListener {
    pub(crate) fn bind(port: u16) -> std::io::Result<Self> {
        let socket = std::net::TcpListener::bind(("127.0.0.1", port))?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            port: socket.local_addr()?.port(),
            socket: Arc::new(Mutex::new(Some(socket))),
            stop: Arc::new(AtomicBool::new(false)),
        })
    }
    pub(crate) fn take_socket(&self) -> std::io::Result<std::net::TcpListener> {
        self.socket
            .lock()
            .map_err(|_| std::io::ErrorKind::Other)?
            .take()
            .ok_or_else(|| std::io::ErrorKind::NotConnected.into())
    }
    pub(crate) fn stop_signal(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }
    pub(crate) fn port(&self) -> u16 {
        self.port
    }
    pub(crate) fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
}

/// One bound or failed listener behind the always-present controller.
pub(crate) enum ListenerState {
    Listening { listener: BoundListener },
    Failed(Box<GatewayFailureReport>),
}

pub(crate) struct GatewayInner {
    pub(crate) state_path: PathBuf,
    pub(crate) state: Mutex<GatewayStateFile>,
    pub(crate) routes: RwLock<BTreeMap<AppKind, ActiveRoute>>,
    pub(crate) activation_lock: Mutex<()>,
    pub(crate) listener: RwLock<ListenerState>,
    /// While set, the serve loop answers new requests with 503 so a port
    /// change can drain in-flight work without cutting long requests.
    pub(crate) maintenance: AtomicBool,
    /// Requests accepted but not yet answered; drains to zero before a
    /// listener swap. Panicking workers release their count via unwind.
    pub(crate) inflight: AtomicUsize,
    pub(crate) websocket_connections: AtomicUsize,
    pub(crate) stopping: AtomicBool,
    /// False when the persisted identity could not be read or safely
    /// recreated. A listener must never serve with a fabricated identity.
    pub(crate) state_available: AtomicBool,
    /// Explains why an intentionally rejected persisted route needs an
    /// explicit client repair even while the listener is healthy.
    pub(crate) repair_reason: Mutex<Option<String>>,
    /// A port-change transaction whose rollback could not preserve an
    /// externally modified file. Kept until explicitly resolved.
    pub(crate) blocked_recovery: Mutex<Option<port_change::BlockedPortChange>>,
    pub(crate) metrics: Arc<GatewayMetrics>,
}

impl GatewayInner {
    /// The loopback base URL derived from the persisted port — the single
    /// address contract, read live so a port change takes effect everywhere.
    pub(crate) fn configured_base_url(&self) -> String {
        format!(
            "http://127.0.0.1:{}",
            self.state
                .lock()
                .map(|state| state.port)
                .unwrap_or(DEFAULT_GATEWAY_PORT)
        )
    }
}

/// Drops the in-flight count when the request handler exits, including via
/// panic unwinding, so a drain can never wait on a leaked counter.
pub(crate) struct InflightGuard(Arc<GatewayInner>);

impl Drop for InflightGuard {
    fn drop(&mut self) {
        self.0.inflight.fetch_sub(1, Ordering::AcqRel);
    }
}

/// One active routed provider as shown on the status page. Deliberately no
/// credentials: the interface resolves the display name from its own profile
/// snapshot using the id.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RouteObservation {
    pub(crate) app: AppKind,
    pub(crate) profile_id: String,
    pub(crate) upstream_protocol: UpstreamProtocol,
}

/// The listener's runtime condition, kept distinct from the configured port.
/// The UI must never present a saved port as an actually listening one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum GatewayStatusKind {
    /// Listening without routed clients.
    Standby,
    /// Listening with at least one active route.
    Running,
    /// The configured port is held by another process.
    PortConflict,
    /// The system refused the bind for a non-conflict reason.
    BindRejected,
    /// Gateway state was lost or replaced while a client still points at the
    /// gateway; providers must be re-applied explicitly.
    NeedsRepair,
    /// A port-change transaction is stuck waiting for an explicit decision.
    RecoveryBlocked,
}

/// The process holding the configured port, when the platform could identify
/// it reliably. An unidentified holder stays `None` — the gateway never kills
/// other processes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GatewayPortProcess {
    pub(crate) pid: u32,
    pub(crate) name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum GatewayFailureKind {
    PortInUse,
    SystemRejected,
    StateUnusable,
}

/// Why the listener is down, with the port, the OS error code, and the
/// identified holder process when available.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GatewayFailureReport {
    pub(crate) port: u16,
    pub(crate) kind: GatewayFailureKind,
    pub(crate) os_code: Option<i32>,
    pub(crate) message: String,
    pub(crate) process: Option<GatewayPortProcess>,
}

/// Read-only listener facts plus request telemetry for the status page.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GatewayObservation {
    /// The saved port that client configurations are built against.
    pub(crate) configured_port: u16,
    /// The port actually being served, or `null` while not listening.
    pub(crate) listening_port: Option<u16>,
    /// The served loopback base URL, or `null` while not listening.
    pub(crate) base_url: Option<String>,
    pub(crate) status: GatewayStatusKind,
    pub(crate) failure: Option<GatewayFailureReport>,
    pub(crate) repair_reason: Option<String>,
    /// Present only while a port-change transaction awaits a decision.
    pub(crate) blocked_recovery: Option<port_change::BlockedPortChange>,
    pub(crate) routes: Vec<RouteObservation>,
    pub(crate) metrics: GatewayMetricsSnapshot,
}

/// The application-managed loopback listener. It is bound exclusively to
/// `127.0.0.1`; the routing catalog is not a network management surface.
#[derive(Clone)]
pub(crate) struct GatewayController {
    pub(crate) inner: Arc<GatewayInner>,
}

impl GatewayController {
    /// The saved port every client-facing address is derived from.
    pub(crate) fn configured_port(&self) -> u16 {
        self.inner
            .state
            .lock()
            .map(|state| state.port)
            .unwrap_or(DEFAULT_GATEWAY_PORT)
    }

    /// The loopback base URL that client configurations are written against.
    /// It follows the configured port, the single address contract, whether
    /// or not the listener is currently up.
    pub(crate) fn configured_base_url(&self) -> String {
        self.inner.configured_base_url()
    }

    pub(crate) fn listening(&self) -> Option<BoundListener> {
        let listener = self.inner.listener.read().ok()?;
        match &*listener {
            ListenerState::Listening { listener }
                if !listener.stop_signal().load(Ordering::Acquire) =>
            {
                Some(listener.clone())
            }
            ListenerState::Listening { .. } => None,
            ListenerState::Failed(_) => None,
        }
    }

    /// The served base URL; `None` while the listener is down.
    pub(crate) fn listening_base_url(&self) -> Option<String> {
        self.listening()
            .map(|listener| format!("http://127.0.0.1:{}", listener.port()))
    }

    #[cfg(test)]
    pub(crate) fn is_listening(&self) -> bool {
        self.listening().is_some()
    }

    pub(crate) fn blocked_recovery(&self) -> Option<port_change::BlockedPortChange> {
        self.inner
            .blocked_recovery
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    pub(crate) fn state_available(&self) -> bool {
        self.inner.state_available.load(Ordering::Acquire)
    }

    /// Persists the one listener-address contract before publishing it to
    /// callers. The port-change journal decides whether a later failure rolls
    /// this fact back or completes it.
    pub(crate) fn persist_port(&self, port: u16) -> Result<(), String> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| "本机协议网关状态锁不可用".to_string())?;
        let mut next = state.clone();
        next.port = port;
        write_state(&self.inner.state_path, &next)?;
        *state = next;
        Ok(())
    }

    /// Publishes a replacement listener and returns the previous listener so
    /// the caller can stop that exact socket after the handoff.
    pub(crate) fn replace_listener(&self, listener: BoundListener) -> Option<BoundListener> {
        // The listener lock protects only this process's publication slot. If
        // a handler panicked while holding it, keeping the poisoned old slot
        // would strand a committed endpoint transaction, so retain its value
        // and complete the deterministic handoff.
        let mut state = self
            .inner
            .listener
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::mem::replace(&mut *state, ListenerState::Listening { listener });
        match previous {
            ListenerState::Listening { listener } => Some(listener),
            ListenerState::Failed(_) => None,
        }
    }

    pub(crate) fn block_port_change(&self, blocked: port_change::BlockedPortChange) {
        if let Ok(mut slot) = self.inner.blocked_recovery.lock() {
            *slot = Some(blocked);
        }
    }

    pub(crate) fn clear_blocked_port_change(&self) -> Result<(), String> {
        let mut slot = self
            .inner
            .blocked_recovery
            .lock()
            .map_err(|_| "端口修改恢复状态锁不可用".to_string())?;
        *slot = None;
        Ok(())
    }

    /// Replaces the listener fact atomically.
    pub(crate) fn set_listener(&self, state: ListenerState) {
        if let Ok(mut listener) = self.inner.listener.write() {
            *listener = state;
        }
    }

    pub(crate) fn shutdown(&self) {
        self.inner.stopping.store(true, Ordering::Release);
        if let Some(listener) = self.listening() {
            listener.stop();
        }
    }
}

/// Reads the client files best-effort and reports whether either still points
/// at a loopback gateway endpoint with this application's capability token.
pub(crate) fn points_at_gateway_files(local: &LocalState) -> bool {
    for app in [AppKind::Codex, AppKind::Claude] {
        let Ok(target) = local.target(app) else {
            continue;
        };
        let Ok(text) = fs::read_to_string(target) else {
            continue;
        };
        if routing::config_points_at_gateway(app, &text) {
            return true;
        }
    }
    false
}

/// A minimal failure report for lock-poisoned or test-only paths.
pub(crate) fn test_failure_report(port: u16, message: &str) -> GatewayFailureReport {
    GatewayFailureReport {
        port,
        kind: GatewayFailureKind::PortInUse,
        os_code: None,
        message: message.to_string(),
        process: None,
    }
}

impl InflightGuard {
    pub(crate) fn try_acquire(inner: Arc<GatewayInner>) -> Option<Self> {
        if inner.maintenance.load(Ordering::Acquire) || inner.stopping.load(Ordering::Acquire) {
            return None;
        }
        inner
            .inflight
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < 8).then_some(count + 1)
            })
            .ok()?;
        let guard = Self(inner);
        if guard.0.maintenance.load(Ordering::Acquire) || guard.0.stopping.load(Ordering::Acquire) {
            return None;
        }
        Some(guard)
    }
}
