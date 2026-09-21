//! Structured resources whose ownership is independent of client preferences.

/// The settings directory owns the preserve-only boundary, including descendants.
/// A terminal named-resource placeholder denotes a table, never an unknown scalar.
pub(crate) fn preserve_only_path(app: crate::AppKind, path: &[String], is_table: bool) -> bool {
    let entries = match app {
        crate::AppKind::Codex => super::directory::CODEX_DIRECTORY_FAMILIES,
        crate::AppKind::Claude => super::directory::CLAUDE_DIRECTORY_FAMILIES,
    };
    entries.iter().filter(|entry| entry.disposition == super::OfficialSettingDisposition::PreserveOnly)
        .flat_map(|entry| std::iter::once(entry.path).chain(entry.related_paths.iter().copied()))
        .any(|pattern| matches_configuration_path(pattern, path, is_table))
}

fn matches_configuration_path(pattern: &str, path: &[String], is_table: bool) -> bool {
    if pattern.contains('/') || pattern.ends_with(".toml") || pattern.ends_with(".json") || pattern.ends_with(".md") {
        return false;
    }
    let segments: Vec<_> = pattern.split('.').collect();
    if path.len() < segments.len() { return false; }
    for (index, (pattern, key)) in segments.iter().zip(path).enumerate() {
        if pattern.starts_with('<') && pattern.ends_with('>') {
            if index + 1 == segments.len() && path.len() == segments.len() && !is_table { return false; }
        } else if let Some((prefix, suffix)) = pattern.split_once('*') {
            if !key.starts_with(prefix) || !key.ends_with(suffix) { return false; }
        } else if *pattern != key.as_str() { return false; }
    }
    true
}

pub fn is_codex_extension_path(path: &str) -> bool {
    matches!(path.split('.').next(), Some("mcp_servers" | "hooks" | "plugins" | "apps"))
        || path == "skills.config"
        || path.starts_with("skills.config.")
}

pub fn is_claude_extension_path(path: &str) -> bool {
    matches!(
        path.split('.').next(),
        Some(
            "mcpServers"
                | "hooks"
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
