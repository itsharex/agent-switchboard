//! Credential-free historical usage ledger.
//!
//! This module is the only owner of the persisted history schema. It records
//! normalized values after successful live reads, never receives a renderer
//! path, and never persists provider credentials, endpoints, raw responses,
//! account identifiers, or query source.

mod ledger;
mod query;
mod record;

#[cfg(test)]
mod tests;

pub(crate) use query::{official_series, provider_series};
pub(crate) use record::{clear_providers, invalidate_provider, record_official, record_provider};
