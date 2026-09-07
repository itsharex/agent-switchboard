//! Manual endpoint probing and the shared outbound HTTP transport.
//!
//! One probe answers exactly one question: does this URL answer HTTP(S)
//! requests, with which status, and how fast. It never selects or switches
//! anything. The transport is [`reqwest`] with rustls verifying against the
//! OS root store, so the system trust and proxy settings apply the way the
//! native stack did. The shared [`http_get`] helper carries that same stack
//! to other outbound reads (model fetch, update check, quota, OAuth).

mod models;
mod transport;

#[cfg(test)]
mod tests;

pub(crate) use models::provider_auth_headers;
pub use models::{fetch_models, ProviderModel};
pub use transport::{http_bytes, http_get, http_request, probe, ProbeResult};
