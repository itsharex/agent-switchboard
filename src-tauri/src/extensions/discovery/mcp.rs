//! Reading native MCP documents and turning their server entries into
//! observations, including Claude's private project sections.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{ExtensionKind, ObservedExtension, ObservedOrigin};
use asb_core::extensions::mcp;

use super::DiscoveryDiagnostic;

pub(super) fn read_discovery_document(
    client: AppKind,
    path: &Path,
    diagnostics: &mut Vec<DiscoveryDiagnostic>,
) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(_) => {
            diagnostics.push(DiscoveryDiagnostic {
                client,
                path: path.to_string_lossy().to_string(),
                message: "无法读取 MCP 配置文档".to_string(),
            });
            None
        }
    }
}

pub(super) fn append_mcp_observations(
    client: AppKind,
    document: &Path,
    origin: ObservedOrigin,
    text: &str,
    out: &mut Vec<ObservedExtension>,
    diagnostics: &mut Vec<DiscoveryDiagnostic>,
) {
    let servers = match client {
        AppKind::Codex => mcp::read_codex_servers(text),
        AppKind::Claude => mcp::read_claude_servers(text, |root| {
            root.get("mcpServers").and_then(|value| value.as_object())
        }),
    };
    let servers = match servers {
        Ok(servers) => servers,
        Err(_) => {
            diagnostics.push(DiscoveryDiagnostic {
                client,
                path: document.to_string_lossy().to_string(),
                message: "MCP 配置文档无法解析；未将其内容当作空配置".to_string(),
            });
            return;
        }
    };
    for server in servers {
        out.push(ObservedExtension {
            kind: ExtensionKind::Mcp,
            client,
            name: server.key.clone(),
            description: None,
            origin: origin.clone(),
            path: document.to_string_lossy().to_string(),
            managed_binding_ids: Vec::new(),
            content_digest: None,
            transport: Some(server.transport.label().to_string()),
            diagnostics: server
                .diagnostics
                .into_iter()
                .chain(
                    server
                        .unknown_fields
                        .into_iter()
                        .map(|field| format!("未识别字段 {field}；按原文保留")),
                )
                .collect(),
        });
    }
}

/// Claude project-private servers live inside the user document, but under
/// the registered project's absolute-path entry rather than the top-level
/// mcpServers object. They are discovered separately so ownership and import
/// can preserve that scope.
pub(super) fn append_claude_project_private_mcp_observations(
    document: &Path,
    project_root: &str,
    text: &str,
    out: &mut Vec<ObservedExtension>,
) {
    let servers = match mcp::read_claude_servers(text, |root| {
        root.get("projects")
            .and_then(|value| value.as_object())
            .and_then(|projects| projects.get(project_root))
            .and_then(|value| value.as_object())
            .and_then(|project| project.get("mcpServers"))
            .and_then(|value| value.as_object())
    }) {
        Ok(servers) => servers,
        // The top-level scan reports malformed user documents once. Do not
        // emit that same diagnostic for every registered project.
        Err(_) => return,
    };
    for server in servers {
        out.push(ObservedExtension {
            kind: ExtensionKind::Mcp,
            client: AppKind::Claude,
            name: server.key.clone(),
            description: None,
            origin: ObservedOrigin::ProjectRoot {
                project_path: project_root.to_string(),
            },
            path: document.to_string_lossy().to_string(),
            managed_binding_ids: Vec::new(),
            content_digest: None,
            transport: Some(server.transport.label().to_string()),
            diagnostics: server
                .diagnostics
                .into_iter()
                .chain(
                    server
                        .unknown_fields
                        .into_iter()
                        .map(|field| format!("未识别字段 {field}；按原文保留")),
                )
                .collect(),
        });
    }
}
