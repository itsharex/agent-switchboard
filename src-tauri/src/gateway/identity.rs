//! Identity and fingerprint helpers for gateway capability tokens.

use super::*;

pub(super) fn is_direct(profile: &ProviderProfile) -> bool {
    !profile.requires_gateway()
}

/// Route fingerprint over the exact profile parameters a loopback route
/// consumes. Application-side metadata (name, notes, website, usage query)
/// does not participate. Claude model fields are included because the gateway
/// applies their mapping at request time; Codex model fields remain owned by
/// its separate catalog revision.
pub(super) fn route_fingerprint(profile: &ProviderProfile) -> Result<String, String> {
    let mut payload = serde_json::json!({
        "app": profile.app,
        "baseUrl": profile.base_url,
        "connection": profile.connection.routing_identity(),
        "apiKey": profile.api_key,
        "upstreamProtocol": profile.upstream_protocol,
        "responsesOptions": profile.responses_options,
        "maxOutputTokens": profile.max_output_tokens.value(),
    });
    if profile.app == AppKind::Claude {
        payload["model"] = serde_json::json!(profile.model);
        payload["modelOptions"] = serde_json::json!(profile.model_options);
    }
    if profile.authentication.is_some_and(|selected| {
        Some(selected)
            != profile
                .upstream_protocol
                .map(UpstreamProtocol::authentication_scheme)
    }) {
        payload["authentication"] = serde_json::json!(profile.authentication);
    }
    let bytes = serde_json::to_vec(&payload).map_err(|_| "无法计算供应商路由指纹".to_string())?;
    Ok(hex_digest(&bytes))
}

pub(crate) fn codex_route_fingerprint(
    file: &asb_core::contracts::CodexProviderFile,
) -> Result<String, String> {
    file.validate()?;
    let profile = &file.profile;
    let payload = serde_json::json!({
        "providerId": profile.id,
        "endpoint": profile.endpoint,
        "apiKey": profile.api_key,
        "authentication": profile.authentication,
        "connection": profile.connection,
        "routeMode": profile.route_mode,
        "upstream": profile.upstream,
        "requestMode": profile.request_mode,
        "defaultModel": profile.default_model,
        "catalog": profile.catalog,
        "modelRoutes": profile.model_routes,
        "subagentRoute": profile.subagent_route,
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
    _profile_id: &str,
    _fingerprint: &str,
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
            hex_digest(format!("asb/claude-capability/v3:{identity}").as_bytes())
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

pub(super) fn continuation_key(
    identity: &str,
    profile: &ProviderProfile,
    route_revision: &str,
) -> [u8; 32] {
    let domain = serde_json::json!([
        "asb/claude-continuation/v3",
        identity,
        profile.id,
        route_revision,
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

pub(crate) fn codex_catalog_file_name(profile_id: &str, revision: &str) -> String {
    format!(
        "agent-switchboard-codex-{profile_id}-{}.json",
        &revision[..16]
    )
}

impl CodexCatalogProjection {
    pub(super) fn from_file(
        state_root: &std::path::Path,
        file: &asb_core::contracts::CodexProviderFile,
        revision: &str,
    ) -> Result<Self, String> {
        file.validate()?;
        let mut catalog = file.profile.catalog.clone();
        if let Some(route) = &file.profile.subagent_route {
            catalog.push(super::routing::subagent_catalog_entry(state_root, route)?);
        }
        let content =
            serde_json::to_string_pretty(&asb_core::contracts::codex_model_catalog_document(
                &catalog,
            ))
            .map_err(|_| "Codex 模型目录序列化失败".to_string())?;
        Ok(Self {
            file_name: codex_catalog_file_name(&file.profile.id, revision),
            content,
        })
    }
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
    fn claude_capability_is_stable_across_provider_revisions() {
        let first = route_token("installation", AppKind::Claude, "provider-a", "revision-a");
        let second = route_token("installation", AppKind::Claude, "provider-b", "revision-b");
        assert_eq!(first, second);
        assert!(first.starts_with("asb_local_"));
        assert_ne!(
            first,
            route_token(
                "another-installation",
                AppKind::Claude,
                "provider-a",
                "revision-a"
            )
        );
    }

    #[test]
    fn claude_continuation_changes_with_the_route_revision() {
        let profile = ProviderProfile {
            id: "provider".into(),
            app: AppKind::Claude,
            route_mode: RouteMode::Custom,
            name: "provider".into(),
            model: None,
            base_url: Some("https://example.test".into()),
            connection: Default::default(),
            api_key: "secret".into(),
            authentication: None,
            upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
            responses_options: None,
            max_output_tokens: None.into(),
            model_options: None,
            parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
            claude_fragment: Default::default(),
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
            display: None,
        };
        assert_ne!(
            continuation_key("installation", &profile, "revision-a"),
            continuation_key("installation", &profile, "revision-b")
        );
    }

    #[test]
    fn claude_model_routing_is_part_of_the_route_revision() {
        let mut first = ProviderProfile {
            id: "provider".into(),
            app: AppKind::Claude,
            route_mode: RouteMode::Custom,
            name: "provider".into(),
            model: Some("default-a".into()),
            base_url: Some("https://example.test".into()),
            connection: Default::default(),
            api_key: "secret".into(),
            authentication: None,
            upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
            responses_options: None,
            max_output_tokens: None.into(),
            model_options: None,
            parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
            claude_fragment: Default::default(),
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
            display: None,
        };
        let second = first.clone();
        first.model = Some("default-b".into());
        assert_ne!(
            route_fingerprint(&first).unwrap(),
            route_fingerprint(&second).unwrap()
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
