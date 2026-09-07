//! Managed-binding consistency diagnostics: the discovery-side producer
//! that compares one binding's deployed target with its baseline and turns
//! every mismatch into the typed, remediation-classed diagnostic the
//! workspace surfaces. Disabled bindings are deliberately inactive and
//! report nothing; their detail view still shows the file state.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use asb_core::contracts::AppKind;
use asb_core::extensions::content_integrity::is_unmodified_subset;
use asb_core::extensions::contracts::{
    DesiredState, ExtensionBinding, ExtensionDefinition, ExtensionPayload, ExtensionTarget,
    ManagedBaseline, ManagedBaselineFile,
};
use asb_core::extensions::diagnostics::{DiagnosticCode, DiagnosticRemediation};
use asb_core::extensions::skill::content_digest;

use super::{DiagnosticSeed, DiagnosticSubjectSeed};

/// The single dispatcher: one managed binding in, at most one diagnostic
/// out. Everything below re-derives its facts from the current disk, so a
/// repair plan built on top of it re-verifies the same preconditions.
pub fn managed_binding_diagnostic(
    binding: &ExtensionBinding,
    definition: &ExtensionDefinition,
    baseline: Option<&ManagedBaselineFile>,
    projects: &[asb_core::extensions::contracts::ProjectRegistration],
) -> Option<DiagnosticSeed> {
    if binding.desired != DesiredState::Enabled {
        return None;
    }
    match &definition.payload {
        ExtensionPayload::Skill(_) => managed_skill_diagnostic(binding, baseline),
        ExtensionPayload::Mcp(_) => managed_mcp_diagnostic(binding, baseline, projects),
    }
}

/// The managed Skill directory: missing files stay auto-repairable, any
/// modification or unknown addition is an external change for a person.
fn managed_skill_diagnostic(
    binding: &ExtensionBinding,
    baseline: Option<&ManagedBaselineFile>,
) -> Option<DiagnosticSeed> {
    let subject = || DiagnosticSubjectSeed::Binding {
        binding_id: binding.id.clone(),
    };
    let Some(ManagedBaseline::Directory {
        target_dir,
        last_files,
        last_digest,
        ..
    }) = baseline.and_then(|file| {
        file.entries
            .iter()
            .find(|entry| matches!(entry, ManagedBaseline::Directory { .. }))
    })
    else {
        return None;
    };
    let dir = Path::new(target_dir);
    match super::skills::walk_skill_dir(dir) {
        Ok(entries) => {
            if content_digest(&entries) == *last_digest {
                return None;
            }
            // Repairable exactly when nothing was modified and nothing
            // unknown appeared: the current content is a subset of the last
            // deployed version. Everything else is an external change a
            // person must resolve.
            let missing_only = is_unmodified_subset(&entries, last_files);
            let name = directory_name(dir);
            Some(if missing_only {
                DiagnosticSeed {
                    code: DiagnosticCode::ManagedTargetMissing,
                    client: binding.target.client(),
                    subject: subject(),
                    message: format!("托管 Skill 目录 {name} 缺少部分已部署文件"),
                    remediation: DiagnosticRemediation::Auto {
                        reason: "可从本地内容库恢复最后一次部署的版本".to_string(),
                    },
                }
            } else {
                DiagnosticSeed {
                    code: DiagnosticCode::ManagedTargetExternalChange,
                    client: binding.target.client(),
                    subject: subject(),
                    message: format!("托管 Skill 目录 {name} 的内容与本应用记录的部署不一致"),
                    remediation: DiagnosticRemediation::Manual {
                        reason: "内容存在未知修改或新增；请先在扩展详情中处理外部变更".to_string(),
                    },
                }
            })
        }
        Err(_) if !dir.exists() => Some(DiagnosticSeed {
            code: DiagnosticCode::ManagedTargetMissing,
            client: binding.target.client(),
            subject: subject(),
            message: format!("托管 Skill 目录 {} 已缺失", directory_name(dir)),
            remediation: DiagnosticRemediation::Auto {
                reason: "本地内容库保存了已部署版本，可整体恢复".to_string(),
            },
        }),
        Err(error) => Some(DiagnosticSeed {
            code: DiagnosticCode::ManagedTargetUnreadable,
            client: binding.target.client(),
            subject: subject(),
            message: format!(
                "托管 Skill 目录 {} 无法读取：{}",
                dir.display(),
                error.message
            ),
            remediation: DiagnosticRemediation::Manual {
                reason: "可能是链接、权限或占用问题；请处理后重新扫描".to_string(),
            },
        }),
    }
}

/// The managed MCP document entry: only a missing entry inside a readable
/// document is a repair candidate; a missing or unreadable document is a
/// matter for the recovery flow.
fn managed_mcp_diagnostic(
    binding: &ExtensionBinding,
    baseline: Option<&ManagedBaselineFile>,
    projects: &[asb_core::extensions::contracts::ProjectRegistration],
) -> Option<DiagnosticSeed> {
    let subject = || DiagnosticSubjectSeed::Binding {
        binding_id: binding.id.clone(),
    };
    let Some(ManagedBaseline::DocumentEntry {
        target_path,
        entry_pointer,
        last_written_value,
        last_document_hash,
        ..
    }) = baseline.and_then(|file| {
        file.entries
            .iter()
            .find(|entry| matches!(entry, ManagedBaseline::DocumentEntry { .. }))
    })
    else {
        return None;
    };
    let document = Path::new(target_path);
    let text = match fs::read_to_string(document) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Some(DiagnosticSeed {
                code: DiagnosticCode::ManagedTargetMissing,
                client: binding.target.client(),
                subject: subject(),
                message: "托管服务所在的配置文档已缺失".to_string(),
                remediation: DiagnosticRemediation::Manual {
                    reason: "文档整体缺失或损坏时请使用历史恢复流程，不会重建空配置".to_string(),
                },
            });
        }
        Err(_) => {
            return Some(DiagnosticSeed {
                code: DiagnosticCode::ManagedTargetUnreadable,
                client: binding.target.client(),
                subject: subject(),
                message: "托管服务所在的配置文档无法读取".to_string(),
                remediation: DiagnosticRemediation::Manual {
                    reason: "请检查文件权限或占用后重新扫描".to_string(),
                },
            });
        }
    };
    // An unparsable document is reported by the scan-level pass; this
    // binding-level check stops here instead of duplicating it.
    let Some(owned_present) = owned_entry_present(&text, binding, entry_pointer, projects) else {
        return None;
    };
    if !owned_present {
        return Some(DiagnosticSeed {
            code: DiagnosticCode::ManagedEntryMissing,
            client: binding.target.client(),
            subject: subject(),
            message: format!("托管服务条目 {entry_pointer} 不在当前配置中"),
            remediation: match last_written_value {
                Some(_) => DiagnosticRemediation::Auto {
                    reason: "基线保存了最后一次写入的服务条目，可原样恢复".to_string(),
                },
                None => DiagnosticRemediation::Manual {
                    reason: "基线没有可恢复的条目值；请重新部署该扩展".to_string(),
                },
            },
        });
    }
    use sha2::Digest;
    let current_hash = sha2::Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if current_hash != *last_document_hash {
        return Some(DiagnosticSeed {
            code: DiagnosticCode::ManagedTargetExternalChange,
            client: binding.target.client(),
            subject: subject(),
            message: "托管服务所在的配置文档在本应用上次写入后发生了外部变更".to_string(),
            remediation: DiagnosticRemediation::Manual {
                reason: "请先在扩展详情中解决外部变更，再继续管理该文档".to_string(),
            },
        });
    }
    None
}

fn directory_name(dir: &Path) -> String {
    dir.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Whether the owned entry currently exists in its document. An unparsable
/// document yields `None` instead of a false "absent".
fn owned_entry_present(
    text: &str,
    binding: &ExtensionBinding,
    entry_pointer: &str,
    projects: &[asb_core::extensions::contracts::ProjectRegistration],
) -> Option<bool> {
    let key = entry_pointer.rsplit('.').next().unwrap_or_default();
    match binding.target.client() {
        AppKind::Codex => asb_core::extensions::mcp::read_codex_servers(text)
            .ok()
            .map(|parsed| parsed.servers.iter().any(|server| server.key == key)),
        AppKind::Claude => {
            // The private-project pointer keeps the raw project path as one
            // segment, so resolution goes through the binding target, never
            // by splitting the pointer text.
            let private_path: Option<String> = match &binding.target {
                ExtensionTarget::ProjectPrivate { .. } => {
                    binding.target.project_id().and_then(|id| {
                        projects
                            .iter()
                            .find(|project| project.id == id)
                            .map(|project| project.root.clone())
                    })
                }
                _ => None,
            };
            asb_core::extensions::mcp::read_claude_servers(text, |root| {
                let mut value = root;
                if let Some(path) = &private_path {
                    value = value.get("projects")?;
                    value = value.get(path.as_str())?;
                }
                value.get("mcpServers")
            })
            .ok()
            .map(|parsed| parsed.servers.iter().any(|server| server.key == key))
        }
    }
}
