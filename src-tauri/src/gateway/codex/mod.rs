//! Codex routing policy and durable state have independent files and commands.
pub(crate) mod health;
pub(crate) mod policy;
mod routes;
pub(crate) mod request;
pub(crate) use routes::CodexEndpointHealth;
