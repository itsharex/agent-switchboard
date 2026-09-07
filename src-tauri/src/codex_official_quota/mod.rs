//! Read-only Codex official-subscription quota service.
//!
//! The service is deliberately separate from provider `UsageQuery`: official
//! quota belongs to the existing Codex ChatGPT login, not to a provider API
//! key or endpoint. OAuth values are read only long enough to make the
//! request and never cross the desktop command boundary.
//!
//! This module additionally owns the persisted-baseline types and the pure
//! after-the-fact reset detection derived from consecutive successful reads.
//! It never writes to disk itself; persisting a baseline is owned by the
//! application-state layer.

mod baseline;
mod service;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use baseline::BaselineRead;
pub(crate) use baseline::{apply_read, CodexQuotaBaseline};
pub(crate) use service::{clear, invalidate, query, query_login};
