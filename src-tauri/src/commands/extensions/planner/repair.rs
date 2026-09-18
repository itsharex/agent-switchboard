//! Repair planning: restoring the last validly deployed state of managed
//! objects the discovery scan found damaged. Repair never deletes, resets,
//! upgrades, or re-renders: skills redeploy exactly their last applied
//! content version, and MCP entries restore the exact last written value.
//! The binding's `lastAppliedRevision` stays where it was.
//!
//! Several repaired MCP entries that live in one document fold into one
//! final document patch: the batch threads the working text through every
//! restoration and a single `DocumentWrite` carries all of them, so
//! unrelated content survives and every baseline lands on the same
//! post-state.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use asb_core::extensions::content_integrity::is_unmodified_subset;
use asb_core::extensions::contracts::PlanOperation;
use asb_core::extensions::contracts::{
    DesiredState, DocumentSyntax, ExtensionBinding, ExtensionDefinition, ExtensionPayload,
    ManagedBaseline, ManagedFileEntry,
};
use asb_core::extensions::mcp::{apply_codex_entry_restore, EntryChange};
use asb_core::extensions::plan::{PlanStep, PlannedFile, PlannedOperation, PlannedTarget};
use asb_core::extensions::skill::content_digest;
use asb_core::extensions::validate::ContentEntryKind;

use crate::commands::error::CommandError;
use crate::commands::extensions::support::*;

use super::deploy::write_entries_to;
use super::document::{read_document, restore_claude_entry, DocumentWork};
use super::{McpScope, Planner};

mod checks;
use checks::entry_present;

/// One fully verified repair, not yet assembled into a plan target.
enum PreparedRepair {
    Skill {
        binding: ExtensionBinding,
        definition_revision: u64,
        target_dir: PathBuf,
        staging_dir: PathBuf,
        planned_files: Vec<PlannedFile>,
        current_digest: Option<String>,
        last_digest: String,
    },
    /// One entry restoration; the document write itself is merged per file.
    McpEntry {
        binding: ExtensionBinding,
        definition_revision: u64,
        document: PathBuf,
        syntax: DocumentSyntax,
        pointer: String,
        value: String,
    },
}

impl PreparedRepair {
    fn definition_revision(&self) -> u64 {
        match self {
            PreparedRepair::Skill {
                definition_revision,
                ..
            }
            | PreparedRepair::McpEntry {
                definition_revision,
                ..
            } => *definition_revision,
        }
    }
}

/// Builds one batch's repair operations: every binding id becomes one
/// verified target, targets of the same definition merge into a single
/// `PlannedOperation`, and all repairs inside one document fold into one
/// final document patch.
pub(crate) fn build_repair_operations(
    planner: &Planner<'_>,
    binding_ids: &[String],
) -> Result<Vec<PlannedOperation>, CommandError> {
    let mut repairs: Vec<PreparedRepair> = Vec::new();
    let mut documents: BTreeMap<String, DocumentWork> = BTreeMap::new();
    for binding_id in binding_ids {
        planner.prepare_repair(binding_id, &mut repairs, &mut documents)?;
    }
    let mut operations: Vec<PlannedOperation> = Vec::new();
    for (index, repair) in repairs.into_iter().enumerate() {
        let definition_revision = repair.definition_revision();
        let target = match repair {
            PreparedRepair::Skill {
                binding,
                target_dir,
                staging_dir,
                planned_files,
                current_digest,
                last_digest,
                ..
            } => {
                let baseline_file = planner.merge_baseline_entry(
                    &binding.id,
                    ManagedBaseline::Directory {
                        target_dir: target_dir.to_string_lossy().to_string(),
                        original_existed: false,
                        original_files: None,
                        original_digest: None,
                        last_files: planned_files
                            .iter()
                            .map(|file| ManagedFileEntry {
                                relative_path: file.relative_path.clone(),
                                digest: file.digest.clone(),
                                mode: file.mode,
                                size: file.size,
                            })
                            .collect(),
                        last_digest,
                        backup_reference: None,
                    },
                )?;
                let client = binding.target.client();
                PlannedTarget {
                    binding_id: Some(binding.id.clone()),
                    target: binding.target.clone(),
                    // last_applied_revision stays; a repair is not a new apply.
                    binding,
                    baseline: Some(baseline_file),
                    steps: vec![PlanStep::DirectoryDeploy {
                        client,
                        target_dir: target_dir.to_string_lossy().to_string(),
                        staging_dir: staging_dir.to_string_lossy().to_string(),
                        files: planned_files,
                        original_digest: current_digest,
                        backup_dir: String::new(),
                    }],
                    warnings: vec![
                        "将恢复最后一次部署的内容版本；版本与停用状态保持不变".to_string()
                    ],
                    changes: Vec::new(),
                    adopts_native_entry: false,
                }
            }
            PreparedRepair::McpEntry {
                mut binding,
                document,
                syntax,
                pointer,
                value,
                ..
            } => {
                let document_key = document.to_string_lossy().to_string();
                let work = documents.get_mut(&document_key).expect("prepared above");
                // The first repair of a document carries the single merged
                // write; later repairs of the same document contribute only
                // their entry and baseline.
                let steps = if work.writer == Some(index) {
                    vec![PlanStep::DocumentWrite {
                        client: work.client,
                        path: document_key.clone(),
                        expected_existed: true,
                        expected_content_hash: work.expected_content_hash.clone(),
                        rendered: work.rendered.clone(),
                        syntax,
                        backup_dir: String::new(),
                    }]
                } else {
                    Vec::new()
                };
                let baseline_file = planner.merge_baseline_entry(
                    &binding.id,
                    ManagedBaseline::DocumentEntry {
                        target_path: document_key.clone(),
                        entry_pointer: pointer.clone(),
                        original_value: None,
                        last_written_value: Some(value.clone()),
                        last_document_hash: sha_hex(work.rendered.as_bytes()),
                        target_existed_before: false,
                        backup_reference: None,
                    },
                )?;
                binding.updated_at = now();
                PlannedTarget {
                    binding_id: Some(binding.id.clone()),
                    target: binding.target.clone(),
                    binding,
                    baseline: Some(baseline_file),
                    steps,
                    warnings: vec!["将原样恢复最后一次写入的服务条目；版本保持不变".to_string()],
                    changes: vec![EntryChange {
                        pointer,
                        before: None,
                        after: Some(value),
                    }],
                    adopts_native_entry: false,
                }
            }
        };
        let resource_id = target.binding.resource_id.clone();
        match operations
            .iter_mut()
            .find(|operation| operation.definition_id == resource_id)
        {
            Some(operation) => operation.targets.push(target),
            None => operations.push(PlannedOperation {
                definition_id: resource_id,
                definition_revision,
                operation: PlanOperation::Repair,
                targets: vec![target],
            }),
        }
    }
    Ok(operations)
}

impl Planner<'_> {
    /// Verifies one binding's repair against the current disk and library
    /// and records it. Every precondition is re-checked here — the scan
    /// that surfaced the diagnostic is only a hint.
    fn prepare_repair(
        &self,
        binding_id: &str,
        repairs: &mut Vec<PreparedRepair>,
        documents: &mut BTreeMap<String, DocumentWork>,
    ) -> Result<(), CommandError> {
        let binding = self
            .store
            .list_bindings()
            .map_err(store_error)?
            .into_iter()
            .find(|binding| binding.id == binding_id)
            .ok_or_else(|| CommandError::new("extension-not-found", "绑定不存在或已被移除"))?;
        let definition = self
            .store
            .get_definition(&binding.resource_id)
            .map_err(store_error)?
            .ok_or_else(|| CommandError::new("extension-not-found", "扩展定义不存在"))?;
        if binding.desired != DesiredState::Enabled {
            return Err(CommandError::new(
                "extension-invalid",
                "已停用的绑定没有需要修复的部署；请重新扫描",
            ));
        }
        self.require_write_capabilities(&definition, &binding, PlanOperation::Repair)?;
        match &definition.payload {
            ExtensionPayload::Skill(_) => {
                let prepared = self.prepare_skill_repair(&definition, &binding)?;
                repairs.push(prepared);
            }
            ExtensionPayload::Mcp(_) => {
                self.prepare_mcp_repair(&binding, &definition, repairs, documents)?;
            }
        }
        Ok(())
    }

    fn prepare_skill_repair(
        &self,
        definition: &ExtensionDefinition,
        binding: &ExtensionBinding,
    ) -> Result<PreparedRepair, CommandError> {
        let target_dir = self.skill_target_dir(binding)?;
        let target = target_dir.to_string_lossy();
        let baseline = self
            .store
            .get_baseline_file(&binding.id)
            .map_err(store_error)?;
        let Some((last_files, last_digest)) = baseline.as_ref().and_then(|file| {
            file.entries.iter().find_map(|entry| match entry {
                ManagedBaseline::Directory {
                    target_dir: dir,
                    last_files,
                    last_digest,
                    ..
                } if dir == target.as_ref() => Some((last_files.clone(), last_digest.clone())),
                _ => None,
            })
        }) else {
            return Err(CommandError::new(
                "extension-baseline",
                "该 Skill 绑定缺少可验证的部署基线，不能自动修复",
            ));
        };
        // The repair version is exactly the last applied one; a version the
        // library can no longer produce verbatim is refused, never faked.
        let entries = self
            .store
            .load_skill_version(&definition.id, &last_digest)
            .map_err(store_error)?;
        let current_entries = self.skill_target_entries(&target_dir)?;
        if let Some(existing) = &current_entries {
            let current_digest = content_digest(existing);
            if current_digest == last_digest {
                return Err(CommandError::new(
                    "extension-conflict",
                    "该目录已与最后一次部署一致；请重新扫描",
                ));
            }
            if !is_unmodified_subset(existing, &last_files) {
                return Err(CommandError::new(
                    "extension-external-change",
                    format!(
                        "{} 存在未知修改或新增内容；请先处理外部变更",
                        target_dir.display()
                    ),
                ));
            }
        }
        let staging_dir = std::env::temp_dir()
            .join("asb-extensions-staging")
            .join(&last_digest);
        write_entries_to(&entries, &staging_dir)?;
        let planned_files: Vec<PlannedFile> = entries
            .iter()
            .map(|entry| PlannedFile {
                relative_path: entry.relative_path.clone(),
                digest: if entry.kind == ContentEntryKind::Dir {
                    String::new()
                } else {
                    sha_hex(&entry.bytes)
                },
                mode: entry.mode,
                size: entry.bytes.len() as u64,
            })
            .collect();
        Ok(PreparedRepair::Skill {
            binding: {
                let mut next = binding.clone();
                next.updated_at = now();
                next
            },
            definition_revision: definition.revision,
            target_dir,
            staging_dir,
            planned_files,
            current_digest: current_entries
                .as_ref()
                .map(|existing| content_digest(existing)),
            last_digest,
        })
    }

    fn prepare_mcp_repair(
        &self,
        binding: &ExtensionBinding,
        definition: &ExtensionDefinition,
        repairs: &mut Vec<PreparedRepair>,
        documents: &mut BTreeMap<String, DocumentWork>,
    ) -> Result<(), CommandError> {
        let target = self.mcp_document(binding)?;
        let document = target.path.clone();
        let syntax = target.syntax;
        let client = binding.target.client();
        // An entry going missing changes the whole-file hash by definition,
        // so the document-level baseline currency check does not apply here.
        // The repair precondition is narrower, exactly as the diagnostic's
        // remediation promises: the document exists, parses, the owned
        // position is free, and the baseline carries the value to restore.
        if !document.exists() {
            return Err(CommandError::new(
                "extension-baseline",
                format!(
                    "{} 已缺失；请使用历史恢复流程，而不是修复单个条目",
                    document.display()
                ),
            ));
        }
        let key = binding
            .native_key
            .as_deref()
            .ok_or_else(|| CommandError::new("extension-invalid", "MCP 绑定缺少服务键"))?;
        let pointer = match &target.scope {
            McpScope::CodexServers => format!("mcp_servers.{key}"),
            McpScope::ClaudeUserServers => format!("mcpServers.{key}"),
            McpScope::ClaudeProjectPrivate { project_path } => {
                format!("projects.{project_path}.mcpServers.{key}")
            }
        };
        let baseline = self
            .store
            .get_baseline_file(&binding.id)
            .map_err(store_error)?;
        let Some(last_written) = baseline.as_ref().and_then(|file| {
            file.entries.iter().find_map(|entry| match entry {
                ManagedBaseline::DocumentEntry {
                    entry_pointer: owned,
                    last_written_value,
                    ..
                } if owned == &pointer => last_written_value.clone(),
                _ => None,
            })
        }) else {
            return Err(CommandError::new(
                "extension-baseline",
                "该服务条目缺少可恢复的基线值；请重新部署该扩展",
            ));
        };
        // One working text per document file, threaded through the batch.
        let document_key = document.to_string_lossy().to_string();
        if !documents.contains_key(&document_key) {
            let text = read_document(&document)?.ok_or_else(|| {
                CommandError::new(
                    "extension-baseline",
                    "配置文档已缺失；请使用历史恢复流程，而不是修复单个条目",
                )
            })?;
            documents.insert(
                document_key.clone(),
                DocumentWork {
                    client,
                    syntax,
                    expected_content_hash: Some(sha_hex(text.as_bytes())),
                    expected_existed: true,
                    rendered: text,
                    writer: None,
                    claimed: BTreeSet::new(),
                },
            );
        }
        let work = documents.get_mut(&document_key).expect("just inserted");
        // Every restoration builds on the batch's progressively patched
        // text, so same-document repairs compose instead of clobbering.
        let base = work.rendered.clone();
        if entry_present(&base, client, &target.scope, key)? {
            return Err(CommandError::new(
                "extension-conflict",
                "该服务条目已在配置中；请重新扫描",
            ));
        }
        let restored = match &target.scope {
            McpScope::CodexServers => {
                apply_codex_entry_restore(&base, key, Some(last_written.as_str()))
                    .map_err(adapter_error)?
            }
            McpScope::ClaudeUserServers => {
                let (rendered, _) =
                    restore_claude_entry(&base, &["mcpServers"], key, &last_written, &pointer)?;
                rendered
            }
            McpScope::ClaudeProjectPrivate { project_path } => {
                let (rendered, _) = restore_claude_entry(
                    &base,
                    &["projects", project_path.as_str(), "mcpServers"],
                    key,
                    &last_written,
                    &pointer,
                )?;
                rendered
            }
        };
        work.rendered = restored;
        if work.writer.is_none() {
            work.writer = Some(repairs.len());
        }
        repairs.push(PreparedRepair::McpEntry {
            binding: {
                let mut next = binding.clone();
                next.updated_at = now();
                next
            },
            definition_revision: definition.revision,
            document,
            syntax,
            pointer: pointer.clone(),
            value: last_written,
        });
        Ok(())
    }
}
