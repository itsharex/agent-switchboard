//! Narrow disk-state checks used only by one-click repair planning.

use asb_core::contracts::AppKind;
use asb_core::extensions::mcp::{read_claude_servers, read_codex_servers};

use crate::commands::error::CommandError;
use crate::commands::extensions::support::adapter_error;

use super::super::McpScope;

/// Whether the exact native slot is occupied. Parsing failures reject the
/// repair instead of treating an unreadable document as a vacant slot.
pub(super) fn entry_present(
    text: &str,
    client: AppKind,
    scope: &McpScope,
    key: &str,
) -> Result<bool, CommandError> {
    let servers = match (client, scope) {
        (AppKind::Codex, McpScope::CodexServers) => {
            read_codex_servers(text).map_err(adapter_error)?.servers
        }
        (AppKind::Claude, McpScope::ClaudeUserServers) => {
            read_claude_servers(text, |root| root.get("mcpServers"))
                .map_err(adapter_error)?
                .servers
        }
        (AppKind::Claude, McpScope::ClaudeProjectPrivate { project_path }) => {
            read_claude_servers(text, |root| {
                root.get("projects")?
                    .get(project_path)
                    .and_then(|project| project.get("mcpServers"))
            })
            .map_err(adapter_error)?
            .servers
        }
        _ => {
            return Err(CommandError::keyed(
                "extension-invalid",
                "errors.extops.repairClientScopeMismatch",
                "修复目标的客户端与配置作用域不匹配",
            ))
        }
    };
    Ok(servers.iter().any(|server| server.key == key))
}
