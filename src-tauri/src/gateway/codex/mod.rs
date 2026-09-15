//! Codex routing policy and durable state have independent files and commands.
pub(crate) mod health;
pub(crate) mod history;
pub(crate) mod media;
pub(crate) mod policy;
pub(crate) mod request;
mod routes;
pub(crate) use routes::CodexEndpointHealth;
