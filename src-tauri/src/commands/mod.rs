//! Typed commands exposed to the UI.
//!
//! The UI passes only profile ids, drafts, app kinds, and preview hashes. The
//! command modules resolve the two supported local configuration paths and
//! are the sole backend entry point for preview, switch, restore, and
//! discovery. Split by domain:
//!
//! - [`error`]: the typed error and shared command guards
//! - [`window`]: title-bar window controls and the inspector toggle
//! - [`status`]: read-only configuration status and lock observation
//! - [`client_settings`]: general-configuration overlay commands
//! - [`official_login`]: official client login start/poll/cancel commands
//! - [`prompt_management`]: global AGENTS.md / CLAUDE.md document commands
//! - [`subagent_settings`]: Codex `[agents]` default-setting commands
//! - [`switching`]: switch, backup, restore, and undo commands
//!
//! Profile store CRUD, application settings, probing, discovery, sessions,
//! and external-profile import live here as thin delegations to their owner modules.
//!
//! Filesystem and database work runs through [`error::blocking`]. Network
//! commands use that pool or cancellable async I/O, keeping the main thread
//! free while a provider responds.

mod app_settings;
pub(crate) mod claude_accounts;
pub(crate) mod claude_endpoints;
pub(crate) mod claude_env;
pub(crate) mod codex_env;
pub(crate) mod claude_failover_import;
pub(crate) mod claude_integration;
pub(crate) mod claude_ledger;
pub(crate) mod claude_management;
pub(crate) mod claude_mcp_import;
pub(crate) mod claude_pricing;
pub(crate) mod claude_prompts;
pub(crate) mod claude_session_usage;
pub(crate) mod codex_accounts;
pub(crate) mod codex_common;
pub(crate) mod codex_endpoints;
pub(crate) mod codex_management;
pub(crate) mod codex_metering;
pub(crate) mod codex_prompts;
mod discovery;
pub(crate) mod failover;
mod profiles;
mod query;
mod quota;

#[cfg(test)]
mod tests;

pub(crate) mod client_settings;
pub(crate) mod cloud_backup;
pub(crate) mod error;
pub(crate) mod extensions;
pub(crate) mod gateway;
pub(crate) mod model_usage;
pub(crate) mod official_login;
pub(crate) mod outbound_proxy;
pub(crate) mod prompt_management;
pub(crate) mod provider_endpoints;
pub(crate) mod provider_request;
pub(crate) mod runtime_log;
pub(crate) mod status;
pub(crate) mod subagent_settings;
pub(crate) mod switching;
pub(crate) mod usage_history;
pub(crate) mod window;

pub(crate) use status::config_status_report;
pub(crate) use status::ConfigFileStatus;
pub(crate) use window::apply_desktop_settings;

use std::sync::{Arc, Mutex, MutexGuard};

/// Serializes the writers that replace client configuration files — switch,
/// save-and-apply, restore/undo, gateway port changes, and retry-driven
/// gateway recovery. Every writer also verifies an expected content hash,
/// but hash checks alone leave a read-then-write window; this gate closes it
/// so two confirmed transactions can never interleave their file writes.
#[derive(Clone, Default)]
pub(crate) struct ConfigWriteGate(Arc<Mutex<()>>);

impl ConfigWriteGate {
    pub(crate) fn shared(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.0)
    }

    pub(crate) fn lock(&self) -> Result<MutexGuard<'_, ()>, String> {
        self.0
            .lock()
            .map_err(|_| "客户端配置写入闸门不可用".to_string())
    }
}

pub use app_settings::*;
pub use discovery::*;
pub use profiles::*;
pub use query::*;
pub use quota::*;
