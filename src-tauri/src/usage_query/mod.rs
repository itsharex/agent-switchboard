//! Profile-owned usage-balance queries.
//!
//! Declarative queries retain fixed GET, the profile-selected credential
//! header, and JSON-Pointer extraction. Script queries evaluate a small
//! JavaScript expression in a fresh, resource-bounded QuickJS context. The
//! script can calculate a request and extract JSON, but receives no host I/O;
//! the shared probe transport remains the only process that performs a
//! network request.

mod declarative;
pub(crate) mod scheduler;
mod script;

#[cfg(test)]
mod tests;

use asb_core::contracts::{UpstreamProtocol, UsageQuery, UsageSummary};
use declarative::run_declarative_query;
use script::{run_script_query, ScriptProgram};

/// Validates every field that becomes durable state. The core validates the
/// shared tagged shape; script mode additionally evaluates the source once to
/// prove it produces both required functions before LocalState writes it.
pub(crate) fn validate_persisted(query: &UsageQuery) -> Result<(), String> {
    asb_core::validate::validate_usage_query(query).map_err(|error| error.to_string())?;
    if let UsageQuery::Script { source, .. } = query {
        ScriptProgram::new(source).map(|_| ())?;
    }
    Ok(())
}

/// Runs one usage query against the provider endpoint. The credential travels
/// only in the declarative mode's selected auth header, or into the script's
/// explicit `request` input. It is never echoed in errors or the returned
/// summary.
pub fn run_usage_query(
    query: &UsageQuery,
    api_key: &str,
    base_url: Option<&str>,
    upstream_protocol: UpstreamProtocol,
) -> Result<UsageSummary, String> {
    asb_core::validate::validate_usage_query(query).map_err(|error| error.to_string())?;
    match query {
        UsageQuery::Declarative {
            url,
            remaining_path,
            used_path,
            total_path,
            unit,
            ..
        } => run_declarative_query(
            url,
            remaining_path.as_deref(),
            used_path.as_deref(),
            total_path.as_deref(),
            unit.clone(),
            api_key,
            base_url,
            upstream_protocol,
        ),
        UsageQuery::Script { source, .. } => run_script_query(source, api_key, base_url),
    }
}
