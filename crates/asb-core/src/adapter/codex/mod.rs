//! Codex `config.toml` adapter.
//!
//! Pure parse / preview / render over configuration text. Host-owned keys,
//! comments and layout are preserved by editing the document through
//! `toml_edit` instead of re-serializing from a typed mirror.

mod document;
mod overlay;
mod preview;
mod render;
mod state;
mod subagents;
#[cfg(test)]
mod tests;

pub(crate) use document::{check_syntax, parse};
pub(crate) use preview::preview;
pub(crate) use render::render_gateway_base_url;
pub(crate) use render::{render, render_client_settings};
pub(crate) use state::{matches_provider_settings, owned_diff};
pub use state::{route_state, OFFICIAL_PROVIDER};
pub use subagents::{deprecated_subagent_keys, read_subagent_settings, render_subagent_settings};

fn validate_projection(
    plan: &crate::contracts::SwitchPlan,
) -> Result<(), crate::adapter::AdapterError> {
    if plan.profile.route_mode == crate::contracts::RouteMode::Custom
        && (!plan.is_gateway() || !plan.client_base_url().is_some_and(is_gateway_base_url))
    {
        return Err(crate::adapter::AdapterError {
            message: "Codex 第三方必须通过本机网关投影".into(),
            line: None,
        });
    }
    Ok(())
}

/// The only endpoint shape a Codex projection may write; it must not send
/// ambient OAuth to an upstream address supplied as a gateway projection.
pub fn is_gateway_base_url(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    let segments: Vec<_> = url.path().split('/').collect();
    url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && url.port().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && matches!(segments.as_slice(), ["", "codex", capability, "v1"]
        if capability.strip_prefix("asb_codex_").is_some_and(|secret| {
            secret.len() == 64 && secret.bytes().all(|c| c.is_ascii_hexdigit())
        }))
}
