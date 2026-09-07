//! The private loopback gateway used only when a provider's upstream wire
//! protocol differs from the selected client's native protocol.
//!
//! A provider profile remains the sole owner of its endpoint and API key. The
//! persisted gateway state stores only a listener identity plus a profile id
//! and fingerprint, so a restart never duplicates an upstream credential.

mod metrics;
mod server;
mod transform;

use metrics::{GatewayMetrics, GatewayMetricsSnapshot};

use crate::local_state::LocalState;
use asb_core::adapter;
use asb_core::contracts::{AppKind, ProviderProfile, RouteMode, SwitchPlan, UpstreamProtocol};
use asb_core::validate_plan;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use tiny_http::Server;
use uuid::Uuid;

mod controller;
mod identity;
mod routing;
mod state;

#[cfg(test)]
mod tests;

use identity::*;
use state::*;

/// The result of resolving a selected provider into the exact client-facing
/// plan. Direct plans retain the provider endpoint; routed plans replace it
/// with one local, per-profile capability token.
#[derive(Clone)]
pub(crate) struct GatewayProjection {
    pub plan: SwitchPlan,
    activation: GatewayActivation,
    warning: Option<String>,
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
    pub(crate) upstream_base_url: String,
    pub(crate) upstream_protocol: UpstreamProtocol,
    pub(crate) max_output_tokens: Option<u64>,
    pub(crate) api_key: String,
}

struct GatewayInner {
    state_path: PathBuf,
    state: Mutex<GatewayStateFile>,
    routes: RwLock<BTreeMap<AppKind, ActiveRoute>>,
    activation_lock: Mutex<()>,
    stopping: AtomicBool,
    base_url: String,
    metrics: Arc<GatewayMetrics>,
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

/// Read-only listener facts plus request telemetry for the status page.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GatewayObservation {
    pub(crate) port: u16,
    pub(crate) base_url: String,
    pub(crate) routes: Vec<RouteObservation>,
    pub(crate) metrics: GatewayMetricsSnapshot,
}

/// The application-managed loopback listener. It is bound exclusively to
/// `127.0.0.1`; the routing catalog is not a network management surface.
#[derive(Clone)]
pub(crate) struct GatewayController {
    inner: Arc<GatewayInner>,
    server: Arc<Server>,
}
