//! Structured resources whose ownership is independent of client preferences.

pub fn is_claude_extension_path(path: &str) -> bool {
    matches!(
        path.split('.').next(),
        Some(
            "mcpServers"
                | "enabledMcpjsonServers"
                | "disabledMcpjsonServers"
                | "enableAllProjectMcpServers"
                | "enabledPlugins"
                | "extraKnownMarketplaces"
                | "skillOverrides"
        )
    )
}

pub fn is_claude_credential_path(path: &str) -> bool {
    matches!(path.split('.').next(), Some("apiKeyHelper" | "otelHeadersHelper"))
}
