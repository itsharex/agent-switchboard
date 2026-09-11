//! Identity and fingerprint helpers for gateway capability tokens.

use super::*;

pub(super) fn is_direct(profile: &ProviderProfile) -> bool {
    !profile.requires_gateway()
}

/// Route fingerprint over the exact profile parameters a loopback route
/// consumes. Application-side metadata (name, notes, website, usage query)
/// and client-side projection fields (model, model options) must not rotate
/// the Codex client capability. The fingerprint is a route revision used for
/// activation, request snapshots, and recovery validation.
pub(super) fn route_fingerprint(profile: &ProviderProfile) -> Result<String, String> {
    let payload = serde_json::json!({
        "app": profile.app,
        "baseUrl": profile.base_url,
        "apiKey": profile.api_key,
        "upstreamProtocol": profile.upstream_protocol,
        "responsesOptions": profile.responses_options,
        "maxOutputTokens": profile.max_output_tokens.value(),
    });
    let bytes = serde_json::to_vec(&payload).map_err(|_| "无法计算供应商路由指纹".to_string())?;
    Ok(hex_digest(&bytes))
}

pub(super) fn codex_route_fingerprint(
    file: &asb_core::contracts::CodexProviderFile,
) -> Result<String, String> {
    file.validate()?;
    let profile = &file.profile;
    let payload = serde_json::json!({
        "providerId": profile.id,
        "endpoint": profile.endpoint,
        "apiKey": profile.api_key,
        "upstream": profile.upstream,
        "requestMode": profile.request_mode,
        "defaultModel": profile.default_model,
        "catalog": profile.catalog,
        "modelRoutes": profile.model_routes,
        "capabilities": profile.capabilities,
        "parameters": file.parameters,
    });
    let bytes =
        serde_json::to_vec(&payload).map_err(|_| "无法计算 Codex 供应商路由修订".to_string())?;
    Ok(hex_digest(&bytes))
}

pub(super) fn route_token(
    identity: &str,
    app: AppKind,
    profile_id: &str,
    fingerprint: &str,
) -> String {
    match app {
        // Codex keeps one stable local entry. The active route revision, not
        // the URL capability, chooses the current third-party provider.
        AppKind::Codex => format!(
            "asb_codex_{}",
            hex_digest(format!("asb/codex-capability/v3:{identity}").as_bytes())
        ),
        AppKind::Claude => format!(
            "asb_local_{}",
            hex_digest(format!("asb/route/v2:{identity}:{profile_id}:{fingerprint}").as_bytes())
        ),
    }
}

pub(super) fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn constant_time_equal(expected: &[u8], received: &[u8]) -> bool {
    let mut different = expected.len() ^ received.len();
    for index in 0..expected.len().max(received.len()) {
        different |=
            usize::from(*expected.get(index).unwrap_or(&0) ^ *received.get(index).unwrap_or(&0));
    }
    different == 0
}

pub(super) fn continuation_key(identity: &str, profile: &ProviderProfile) -> [u8; 32] {
    let domain = serde_json::json!([
        "asb/continuation/v2",
        identity,
        profile.id,
        profile.app,
        profile.base_url,
        profile.upstream_protocol
    ]);
    Sha256::digest(domain.to_string().as_bytes()).into()
}

/// Derives the Codex continuation domain from the complete route revision.
/// The client capability is intentionally stable while providers are hot
/// switched, so it cannot be used as the boundary for encrypted history.
pub(super) fn codex_continuation_key(
    identity: &str,
    profile_id: &str,
    route_revision: &str,
) -> [u8; 32] {
    let domain = serde_json::json!([
        "asb/codex-continuation/v3",
        identity,
        profile_id,
        route_revision,
    ]);
    Sha256::digest(domain.to_string().as_bytes()).into()
}

impl ActiveRoute {
    pub(crate) fn client_endpoint(&self, gateway_base: &str) -> String {
        match self.app {
            AppKind::Codex => format!("{gateway_base}/codex/{}/v1", self.client_token),
            AppKind::Claude => gateway_base.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_capability_is_stable_across_provider_revisions() {
        let first = route_token("installation", AppKind::Codex, "provider-a", "revision-a");
        let second = route_token("installation", AppKind::Codex, "provider-b", "revision-b");
        assert_eq!(first, second);
        assert!(first.starts_with("asb_codex_"));
        assert_ne!(
            first,
            route_token(
                "another-installation",
                AppKind::Codex,
                "provider-a",
                "revision-a"
            )
        );
    }

    #[test]
    fn claude_capability_keeps_its_route_scope() {
        assert_ne!(
            route_token("installation", AppKind::Claude, "provider-a", "revision-a"),
            route_token("installation", AppKind::Claude, "provider-b", "revision-b")
        );
    }

    #[test]
    fn codex_continuation_changes_with_the_route_revision() {
        let first = codex_continuation_key("installation", "provider", "revision-a");
        let second = codex_continuation_key("installation", "provider", "revision-b");
        assert_ne!(first, second);
        assert_ne!(
            first,
            codex_continuation_key("another-installation", "provider", "revision-a")
        );
    }
}
