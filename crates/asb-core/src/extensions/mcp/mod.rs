//! MCP server reading and projection for Codex (`config.toml`) and Claude
//! Code (`~/.claude.json`, `.mcp.json`, project documents).
//!
//! Reading produces observations: recognized fields, transport, native
//! enablement, and every unrecognized field name, so imports can be honest
//! about what they did not understand. Projection renders the typed shared
//! model into each client's native shape. Only explicitly mapped fields
//! cross clients; anything else is refused with a reason instead of being
//! silently dropped. All functions are pure text transforms.

mod claude_launcher;
mod claude_patch;
mod codex_patch;
mod import;
mod observe;
mod redaction;
mod render;
mod skill_rules;
#[cfg(test)]
mod tests;

pub use claude_launcher::{
    unwrap_windows_launcher, wrap_windows_launcher, ClaudeHost, WINDOWS_SHELL_LAUNCHERS,
};
pub use claude_patch::{
    apply_claude_entry_restore, apply_claude_project_disabled_members,
    apply_claude_project_private_server_patches, apply_claude_project_server_patches,
    apply_claude_user_server_patches, claude_project_disabled_member_present,
};
pub use codex_patch::{apply_codex_entry_restore, apply_codex_server_patches, EntryChange};
pub use import::{
    import_claude_project_private_server, import_claude_server, import_codex_server,
    NativeMcpImportError,
};
pub use observe::{
    read_claude_servers, read_codex_servers, McpCollectionProblem, McpEntryProblem,
    ObservedMcpDocument, ObservedMcpServer, ObservedTransport,
};
pub use redaction::{redact_change, redact_rendered_entry};
pub use render::{
    render_claude, render_codex, ClaudeServerRender, CodexServerRender, ProjectionError,
    SecretResolve,
};
pub use skill_rules::{
    apply_claude_skill_override_restore, apply_claude_skill_overrides,
    apply_codex_skill_rule_restore, apply_codex_skill_rules, read_codex_skill_rules,
    CodexSkillRule, SkillOverrideValue,
};

use crate::contracts::AppKind;

/// Returns the native document a client uses for MCP user scope, expressed
/// as a stable label for previews.
pub fn document_label(client: AppKind) -> &'static str {
    match client {
        AppKind::Codex => "config.toml 的 mcp_servers",
        AppKind::Claude => "~/.claude.json 的 mcpServers",
    }
}
