//! Reading native MCP documents and turning their server entries into
//! observations, including Claude's private project sections.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{ExtensionKind, ObservedExtension, ObservedOrigin};
use asb_core::extensions::diagnostics::{DiagnosticCode, DiagnosticRemediation};
use asb_core::extensions::mcp::{self, McpCollectionProblem, McpEntryProblem};

use super::{DiagnosticSeed, DiagnosticSubjectSeed};

pub(super) fn read_discovery_document(
    client: AppKind,
    label: &str,
    path: &Path,
    diagnostics: &mut Vec<DiagnosticSeed>,
) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(_) => {
            diagnostics.push(DiagnosticSeed {
                code: DiagnosticCode::McpDocumentUnreadable,
                client,
                subject: DiagnosticSubjectSeed::Location {
                    label: label.to_string(),
                    location_key: path.to_string_lossy().to_string(),
                    resource_kind: ExtensionKind::Mcp,
                },
                message: "无法读取 MCP 配置文档".to_string(),
                remediation: DiagnosticRemediation::Manual {
                    reason: "请检查文件权限或占用后重新扫描".to_string(),
                },
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
    diagnostics: &mut Vec<DiagnosticSeed>,
) {
    let parsed = match client {
        AppKind::Codex => mcp::read_codex_servers(text),
        AppKind::Claude => mcp::read_claude_servers(text, |root| root.get("mcpServers")),
    };
    let parsed = match parsed {
        Ok(parsed) => parsed,
        Err(_) => {
            diagnostics.push(DiagnosticSeed {
                code: DiagnosticCode::McpDocumentUnparsable,
                client,
                subject: DiagnosticSubjectSeed::Location {
                    label: "MCP 配置文档".to_string(),
                    location_key: document.to_string_lossy().to_string(),
                    resource_kind: ExtensionKind::Mcp,
                },
                message: "MCP 配置文档无法解析；未将其内容当作空配置".to_string(),
                remediation: DiagnosticRemediation::Manual {
                    reason: "请先修复文档语法或使用历史恢复流程；本应用不会重建空配置".to_string(),
                },
            });
            return;
        }
    };
    if parsed.collection_problem == McpCollectionProblem::InvalidType {
        // The collection exists but its type is wrong: report it instead of
        // pretending the document has no servers.
        diagnostics.push(DiagnosticSeed {
            code: DiagnosticCode::McpCollectionInvalid,
            client,
            subject: DiagnosticSubjectSeed::Location {
                label: "MCP 配置文档".to_string(),
                location_key: document.to_string_lossy().to_string(),
                resource_kind: ExtensionKind::Mcp,
            },
            message: "MCP 集合存在但类型错误，无法按服务列表解读".to_string(),
            remediation: DiagnosticRemediation::Manual {
                reason: "请人工修正该集合的结构后重新扫描".to_string(),
            },
        });
    }
    for server in parsed.servers {
        push_server_observation(
            client,
            document.to_string_lossy().as_ref(),
            origin.clone(),
            server,
            out,
            diagnostics,
        );
    }
}

fn push_server_observation(
    client: AppKind,
    document: &str,
    origin: ObservedOrigin,
    server: mcp::ObservedMcpServer,
    out: &mut Vec<ObservedExtension>,
    diagnostics: &mut Vec<DiagnosticSeed>,
) {
    let problem = match &server.problem {
        McpEntryProblem::None => None,
        McpEntryProblem::NotAnObject => Some((
            DiagnosticCode::McpEntryNotAnObject,
            "该条目不是对象，无法解读".to_string(),
            "请人工修正该条目的结构后重新扫描".to_string(),
        )),
        McpEntryProblem::TransportMissing => Some((
            DiagnosticCode::McpTransportMissing,
            "该服务缺少 url 或 command，传输类型无法判定".to_string(),
            "请编辑配置补齐传输字段；本应用不会虚构传输参数".to_string(),
        )),
        McpEntryProblem::TransportConflicting => Some((
            DiagnosticCode::McpTransportConflicting,
            "该服务同时携带 url 与 command，传输字段冲突".to_string(),
            "请人工确认应保留的传输方式后重新扫描".to_string(),
        )),
        McpEntryProblem::UnknownTransportType { type_name } => Some((
            DiagnosticCode::McpTransportUnknown,
            format!("未知传输类型 {type_name}；按只读展示"),
            "请确认该服务的传输类型后重新扫描".to_string(),
        )),
    };
    let index = out.len();
    out.push(ObservedExtension {
        kind: ExtensionKind::Mcp,
        client,
        name: server.key.clone(),
        description: None,
        origin,
        path: document.to_string(),
        managed_binding_ids: Vec::new(),
        content_digest: None,
        transport: Some(server.transport.label().to_string()),
    });
    if let Some((code, message, reason)) = problem {
        diagnostics.push(DiagnosticSeed {
            code,
            client,
            subject: DiagnosticSubjectSeed::Entry {
                observation_index: index,
            },
            message,
            remediation: DiagnosticRemediation::Manual { reason },
        });
    }
    if !server.unknown_fields.is_empty() {
        diagnostics.push(DiagnosticSeed {
            code: DiagnosticCode::McpUnknownFields,
            client,
            subject: DiagnosticSubjectSeed::Entry {
                observation_index: index,
            },
            message: format!(
                "未识别字段 {}；按原文保留",
                server.unknown_fields.join("、")
            ),
            remediation: DiagnosticRemediation::Info,
        });
    }
    for note in server.notes {
        diagnostics.push(DiagnosticSeed {
            code: DiagnosticCode::McpIgnoredField,
            client,
            subject: DiagnosticSubjectSeed::Entry {
                observation_index: index,
            },
            message: note,
            remediation: DiagnosticRemediation::Info,
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
    diagnostics: &mut Vec<DiagnosticSeed>,
) {
    let parsed = match mcp::read_claude_servers(text, |root| {
        root.get("projects")
            .and_then(|value| value.as_object())
            .and_then(|projects| projects.get(project_root))
            .and_then(|value| value.as_object())
            .and_then(|project| project.get("mcpServers"))
    }) {
        Ok(parsed) => parsed,
        // The top-level scan reports malformed user documents once. Do not
        // emit that same diagnostic for every registered project.
        Err(_) => return,
    };
    if parsed.collection_problem == McpCollectionProblem::InvalidType {
        diagnostics.push(DiagnosticSeed {
            code: DiagnosticCode::McpCollectionInvalid,
            client: AppKind::Claude,
            subject: DiagnosticSubjectSeed::Location {
                label: "项目 MCP 配置文档".to_string(),
                location_key: format!("{}#{project_root}", document.display()),
                resource_kind: ExtensionKind::Mcp,
            },
            message: "该项目的 mcpServers 集合存在但类型错误，无法按服务列表解读".to_string(),
            remediation: DiagnosticRemediation::Manual {
                reason: "请人工修正该集合的结构后重新扫描".to_string(),
            },
        });
    }
    for server in parsed.servers {
        push_server_observation(
            AppKind::Claude,
            document.to_string_lossy().as_ref(),
            ObservedOrigin::ProjectRoot {
                project_path: project_root.to_string(),
            },
            server,
            out,
            diagnostics,
        );
    }
}
